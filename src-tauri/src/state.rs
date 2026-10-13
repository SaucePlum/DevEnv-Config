//! 单条目状态机、事件载荷、占位符参数与两类编排（安装 / 配置）。
//! 设计依据：设计文档第 7 节数据流、第 12 节状态机。

use crate::error::{AppError, AppResult};
use crate::manifest::{Item, Kind, Manifest, Scope, Source};
use crate::{config, deploy, detect, download, env, installer, payload, provider, verify};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

/// 界面阶段。与设计文档第 12 节状态机一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    NotInstalled,
    Installed,
    Drift,
    Downloading,
    VerifyingHash,
    WaitingUser,
    WizardRunning,
    Extracting,
    Moving,
    Finalizing,
    Verifying,
    Done,
    Failed,
    NotConfigured,
    Configured,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemEvent {
    pub id: String,
    pub phase: Phase,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempted: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub ts: String,
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub message: String,
}

/// 运行期占位符参数：由前端表单或目录选择器提供。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Params {
    #[serde(default)] pub install_dir: Option<String>,
    #[serde(default)] pub git_name: Option<String>,
    #[serde(default)] pub git_email: Option<String>,
    #[serde(default)] pub prefix_dir: Option<String>,
    #[serde(default)] pub cache_dir: Option<String>,
    #[serde(flatten)] pub extra: BTreeMap<String, String>,
}

impl Params {
    /// 替换 `{install_dir}` / `{git_name}` / `{mirror_url}` 等占位符。
    pub fn fill(&self, s: &str) -> String {
        let mut out = s.to_string();
        let pairs: [(&str, Option<&str>); 5] = [
            ("{install_dir}", self.install_dir.as_deref()),
            ("{git_name}", self.git_name.as_deref()),
            ("{git_email}", self.git_email.as_deref()),
            ("{prefix_dir}", self.prefix_dir.as_deref()),
            ("{cache_dir}", self.cache_dir.as_deref()),
        ];
        for (k, v) in pairs {
            if let Some(v) = v {
                out = out.replace(k, v);
            }
        }
        for (k, v) in &self.extra {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        out
    }
}

fn now_str() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

pub fn emit_state(app: &AppHandle, id: &str, phase: Phase, message: impl Into<String>, progress: Option<f64>) {
    let _ = app.emit(
        "item-state",
        ItemEvent { id: id.to_string(), phase, message: message.into(), progress, attempted: None, exit_code: None },
    );
}

pub fn emit_state_full(app: &AppHandle, ev: ItemEvent) {
    let _ = app.emit("item-state", ev);
}

pub fn log(app: &AppHandle, level: &str, item: Option<&str>, message: impl Into<String>) {
    let _ = app.emit(
        "log",
        LogEntry {
            ts: now_str(),
            level: level.to_string(),
            item: item.map(String::from),
            message: message.into(),
        },
    );
}

/// 安装 / 投放入口。`version` 为空表示固定项（VC、VS）或内置项。
pub async fn start_item(app: AppHandle, manifest: Manifest, id: String, version: Option<String>, params: Params) -> AppResult<()> {
    let item = manifest.get(&id).cloned().ok_or_else(|| AppError::Other(format!("未知条目 {id}")))?;

    if item.kind == Kind::Config {
        return Err(AppError::Other("配置项请走 run_config".into()));
    }

    // 1. 取到安装包 / 压缩包路径
    emit_state(&app, &id, Phase::Downloading, "准备资源", Some(0.02));
    let mut downloaded: Vec<PathBuf> = Vec::new();
    let local_path: PathBuf = match item.source {
        Source::Local => {
            let p = payload::resolve_local(&app, item.path.as_deref().ok_or_else(|| AppError::MissingResource(id.clone()))?)?;
            log(&app, "info", Some(&id), format!("使用内置资源 {}", p.display()));
            p
        }
        Source::Online => {
            let ver = version.clone().unwrap_or_else(|| "最新".into());
            let targets = provider::resolve(&item, &ver).await.map_err(|e| AppError::Provider(e.to_string()))?;
            for t in &targets {
                log(&app, "info", Some(&id), format!("下载 {}", t.file_name));
                for (i, u) in t.urls.iter().enumerate() {
                    log(&app, "debug", Some(&id), format!("  候选地址 {}: {u}", i + 1));
                }
                let aid = id.clone();
                let app2 = app.clone();
                // 进度节流：100ms 内的重复事件丢弃（完成时必发），避免事件洪泛
                let last_ms = AtomicU64::new(0);
                let report = download::fetch(t, &move |done, total| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    let finished = total.is_some_and(|t| done >= t);
                    if !finished && now.saturating_sub(last_ms.load(Ordering::Relaxed)) < 100 {
                        return;
                    }
                    last_ms.store(now, Ordering::Relaxed);
                    let p = total.map(|t| done as f64 / t as f64);
                    emit_state(&app2, &aid, Phase::Downloading, "下载中", p);
                })
                .await
                .inspect_err(|e| emit_state(&app, &id, Phase::Failed, e.to_string(), None))?;
                if report.reused_cache {
                    log(&app, "info", Some(&id), "命中缓存，跳过下载");
                }
                emit_state(&app, &id, Phase::VerifyingHash, "校验文件", Some(0.9));
                downloaded.push(report.path);
            }
            downloaded
                .last()
                .cloned()
                .ok_or_else(|| AppError::Download("没有可下载的文件".into()))?
        }
        Source::Config => unreachable!(),
    };

    match item.kind {
        Kind::Installer => {
            emit_state(&app, &id, Phase::WaitingUser, "请在安装向导中完成安装", None);
            let path = local_path.clone();
            let app2 = app.clone();
            let id2 = id.clone();
            let code = tauri::async_runtime::spawn_blocking(move || installer::run_and_wait(&path, None))
                .await
                .map_err(|e| AppError::Other(e.to_string()))??;
            log(&app2, "info", Some(&id2), format!("安装器退出码 {code}"));
            emit_state_full(&app2, ItemEvent { id: id2.clone(), phase: Phase::WizardRunning, message: format!("安装器退出码 {code}"), progress: None, attempted: None, exit_code: Some(code) });
            if code != 0 {
                emit_state(&app2, &id2, Phase::Failed, format!("安装器退出码 {code}"), None);
                return Err(AppError::InstallerExit(code));
            }
            // 固定项可能对应多个安装包（VC 运行库有 x64 与 x86 两个），依次导向导。
            // downloaded 仅在 Source::Online 分支填充，本地单文件项此处为空，不会进入循环。
            for p2 in downloaded.iter().skip(1) {
                emit_state(
                    &app,
                    &id,
                    Phase::WaitingUser,
                    format!("请在向导中完成 {} 的安装", p2.file_name().unwrap_or_default().to_string_lossy()),
                    None,
                );
                let p3 = p2.clone();
                let code2 = tauri::async_runtime::spawn_blocking(move || installer::run_and_wait(&p3, None))
                    .await
                    .map_err(|e| AppError::Other(e.to_string()))??;
                log(&app, "info", Some(&id), format!("{} 安装器退出码 {code2}", p2.display()));
                if code2 != 0 {
                    emit_state(&app, &id, Phase::Failed, format!("{} 退出码 {code2}", p2.display()), None);
                    return Err(AppError::InstallerExit(code2));
                }
            }
        }
        Kind::Portable => {
            let install_dir = params
                .install_dir
                .clone()
                .ok_or_else(|| AppError::Config("免安装类需要提供安装目录".into()))?;
            let dir = PathBuf::from(&install_dir);
            let archive = local_path.clone();
            let app2 = app.clone();
            let id2 = id.clone();
            emit_state(&app2, &id2, Phase::Extracting, "解压中", Some(0.1));
            tauri::async_runtime::spawn_blocking(move || {
                deploy::deploy_archive(&archive, &dir, &id2, &|msg, p| {
                    let phase = if msg.contains("移动") { Phase::Moving } else { Phase::Extracting };
                    emit_state(&app2, &id2, phase, msg, p);
                })
            })
            .await
            .map_err(|e| AppError::Other(e.to_string()))??;
        }
        Kind::Config => unreachable!(),
    }

    // 2. 收尾：环境变量
    emit_state(&app, &id, Phase::Finalizing, "配置环境变量", Some(0.95));
    finalize_env(&app, &item, &params)?;

    // 3. 验证
    emit_state(&app, &id, Phase::Verifying, "验证中", Some(0.98));
    let item2 = item.clone();
    let params2 = params.clone();
    let out = tauri::async_runtime::spawn_blocking(move || verify::run_verify(&item2, &params2, &[]))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    if out.ok {
        log(&app, "info", Some(&id), format!("验证通过：{}", out.output.lines().next().unwrap_or("")));
        emit_state(&app, &id, Phase::Done, "已完成", Some(1.0));
        Ok(())
    } else {
        log(&app, "warn", Some(&id), format!("验证未通过（退出码 {}）：{}", out.code, out.output));
        emit_state(&app, &id, Phase::Failed, "验证未通过，可手动核对", Some(1.0));
        Err(AppError::Verify(out.output))
    }
}

/// 收尾环境变量：安装类的 PATH 追加写系统级，写前先做规范化检测。
fn finalize_env(app: &AppHandle, item: &Item, params: &Params) -> AppResult<()> {
    let env_action = match &item.env {
        Some(e) => e,
        None => return Ok(()),
    };
    for dir in &env_action.path_append {
        let d = params.fill(dir);
        match env::ensure_path_entry(Scope::System, &d)? {
            env::PathWrite::Skipped => log(app, "info", Some(&item.id), format!("PATH 已由安装器配置，跳过：{d}")),
            env::PathWrite::Appended => log(app, "info", Some(&item.id), format!("PATH 已追加：{d}")),
        }
    }
    for (k, v) in &env_action.set {
        env::set_var(Scope::System, &params.fill(k), &params.fill(v))?;
    }
    env::broadcast_change();
    Ok(())
}

/// 执行配置项：先检查依赖，再备份、执行动作、复查。
pub async fn run_config(app: AppHandle, manifest: Manifest, id: String, mirror_url: Option<String>, params: Params) -> AppResult<Vec<config::StepLog>> {
    let item = manifest.get(&id).cloned().ok_or_else(|| AppError::Other(format!("未知配置项 {id}")))?;
    if item.kind != Kind::Config {
        return Err(AppError::Other(format!("{id} 不是配置项")));
    }

    // 依赖检查
    for dep in &item.requires {
        let dep_item = manifest.get(dep).ok_or_else(|| AppError::Other(format!("缺少依赖项 {dep}")))?;
        let r = detect::detect_item(dep_item);
        if !matches!(r.status, detect::DetectStatus::Installed | detect::DetectStatus::Configured) {
            let msg = format!("需先安装 {}", dep_item.name);
            emit_state(&app, &id, Phase::Failed, &msg, None);
            return Err(AppError::Config(msg));
        }
    }

    emit_state(&app, &id, Phase::NotConfigured, "备份并执行", Some(0.2));
    let path = env::backup_env(&format!("{}（配置前）", item.name))?;
    log(&app, "info", Some(&id), format!("已备份环境变量：{}", path.display()));

    let item2 = item.clone();
    let params2 = params.clone();
    emit_finalizing(&app, &id, "开始执行配置动作", 0.4);
    let logs = tauri::async_runtime::spawn_blocking(move || config::run_actions(&item2, mirror_url.as_deref(), &params2))
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    for l in &logs {
        log(&app, if l.ok { "info" } else { "warn" }, Some(&id), l.message.clone());
    }

    emit_state(&app, &id, Phase::Verifying, "复查中", Some(0.9));
    let item3 = item.clone();
    let out = tauri::async_runtime::spawn_blocking(move || detect::detect_item(&item3))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    if out.status == detect::DetectStatus::Configured {
        emit_state(&app, &id, Phase::Configured, "已配置", Some(1.0));
    } else {
        emit_state(&app, &id, Phase::NotConfigured, "执行完成，但复查未确认", Some(1.0));
    }
    Ok(logs)
}

fn emit_finalizing(app: &AppHandle, id: &str, msg: &str, p: f64) {
    emit_state(app, id, Phase::Finalizing, msg, Some(p));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_fill_replaces_known_placeholders() {
        let p = Params {
            install_dir: Some("D:\\Tools\\redis".into()),
            git_email: Some("me@example.com".into()),
            ..Default::default()
        };
        assert_eq!(p.fill("{install_dir}\\bin"), "D:\\Tools\\redis\\bin");
        assert_eq!(p.fill("ssh-keygen -C {git_email}"), "ssh-keygen -C me@example.com");
    }

    #[test]
    fn params_fill_keeps_unknown_placeholder() {
        let p = Params::default();
        assert_eq!(p.fill("{mirror_url}/rustup"), "{mirror_url}/rustup");
    }

    #[test]
    fn params_extra_placeholders_work() {
        let mut p = Params::default();
        p.extra.insert("custom".into(), "X".into());
        assert_eq!(p.fill("a-{custom}"), "a-X");
    }
}
