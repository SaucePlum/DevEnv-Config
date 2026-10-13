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
    let item = manifest.get(&id).cloned().ok_or_else(|| AppError::Other(crate::tr!("未知条目 {0}", "Unknown item {0}", id)))?;

    if item.kind == Kind::Config {
        return Err(AppError::Other(crate::tr!("配置项请走 run_config", "Config items must go through run_config").into()));
    }

    // 1. 取到安装包 / 压缩包路径
    emit_state(&app, &id, Phase::Downloading, crate::tr!("准备资源", "Preparing resources"), Some(0.02));
    let mut downloaded: Vec<PathBuf> = Vec::new();
    let local_path: PathBuf = match item.source {
        Source::Local => {
            let p = payload::resolve_local(&app, item.path.as_deref().ok_or_else(|| AppError::MissingResource(id.clone()))?)?;
            log(&app, "info", Some(&id), crate::tr!("使用内置资源 {0}", "Using bundled resource {0}", p.display()));
            p
        }
        Source::Online => {
            let ver = version.clone().unwrap_or_else(|| crate::tr!("最新", "latest"));
            let targets = provider::resolve(&item, &ver).await.map_err(|e| AppError::Provider(e.to_string()))?;
            for t in &targets {
                log(&app, "info", Some(&id), crate::tr!("下载 {0}", "Downloading {0}", t.file_name));
                for (i, u) in t.urls.iter().enumerate() {
                    log(&app, "debug", Some(&id), crate::tr!("  候选地址 {0}: {1}", "  Candidate URL {0}: {1}", i + 1, u));
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
                    emit_state(&app2, &aid, Phase::Downloading, crate::tr!("下载中", "Downloading"), p);
                })
                .await
                .inspect_err(|e| emit_state(&app, &id, Phase::Failed, e.to_string(), None))?;
                if report.reused_cache {
                    log(&app, "info", Some(&id), crate::tr!("命中缓存，跳过下载", "Cache hit, skipping download"));
                }
                emit_state(&app, &id, Phase::VerifyingHash, crate::tr!("校验文件", "Verifying file"), Some(0.9));
                downloaded.push(report.path);
            }
            downloaded
                .last()
                .cloned()
                .ok_or_else(|| AppError::Download(crate::tr!("没有可下载的文件", "No downloadable file").into()))?
        }
        Source::Config => unreachable!(),
    };

    match item.kind {
        Kind::Installer => {
            emit_state(&app, &id, Phase::WaitingUser, crate::tr!("请在安装向导中完成安装", "Complete the installation in the setup wizard"), None);
            let path = local_path.clone();
            let app2 = app.clone();
            let id2 = id.clone();
            let code = tauri::async_runtime::spawn_blocking(move || installer::run_and_wait(&path, None))
                .await
                .map_err(|e| AppError::Other(e.to_string()))??;
            log(&app2, "info", Some(&id2), crate::tr!("安装器退出码 {0}", "Installer exit code {0}", code));
            emit_state_full(&app2, ItemEvent { id: id2.clone(), phase: Phase::WizardRunning, message: crate::tr!("安装器退出码 {0}", "Installer exit code {0}", code), progress: None, attempted: None, exit_code: Some(code) });
            if code != 0 {
                emit_state(&app2, &id2, Phase::Failed, crate::tr!("安装器退出码 {0}", "Installer exit code {0}", code), None);
                return Err(AppError::InstallerExit(code));
            }
            // 固定项可能对应多个安装包（VC 运行库有 x64 与 x86 两个），依次导向导。
            // downloaded 仅在 Source::Online 分支填充，本地单文件项此处为空，不会进入循环。
            for p2 in downloaded.iter().skip(1) {
                emit_state(
                    &app,
                    &id,
                    Phase::WaitingUser,
                    crate::tr!("请在向导中完成 {0} 的安装", "Complete the installation of {0} in the wizard", p2.file_name().unwrap_or_default().to_string_lossy()),
                    None,
                );
                let p3 = p2.clone();
                let code2 = tauri::async_runtime::spawn_blocking(move || installer::run_and_wait(&p3, None))
                    .await
                    .map_err(|e| AppError::Other(e.to_string()))??;
                log(&app, "info", Some(&id), crate::tr!("{0} 安装器退出码 {1}", "{0} installer exit code {1}", p2.display(), code2));
                if code2 != 0 {
                    emit_state(&app, &id, Phase::Failed, crate::tr!("{0} 退出码 {1}", "{0} exit code {1}", p2.display(), code2), None);
                    return Err(AppError::InstallerExit(code2));
                }
            }
        }
        Kind::Portable => {
            let install_dir = params
                .install_dir
                .clone()
                .ok_or_else(|| AppError::Config(crate::tr!("免安装类需要提供安装目录", "A portable item requires an install directory").into()))?;
            let dir = PathBuf::from(&install_dir);
            let archive = local_path.clone();
            let app2 = app.clone();
            let id2 = id.clone();
            emit_state(&app2, &id2, Phase::Extracting, crate::tr!("解压中", "Extracting"), Some(0.1));
            tauri::async_runtime::spawn_blocking(move || {
                deploy::deploy_archive(&archive, &dir, &id2, &|stage, msg, p| {
                    let phase = match stage {
                        deploy::DeployStage::Moving => Phase::Moving,
                        _ => Phase::Extracting,
                    };
                    emit_state(&app2, &id2, phase, msg, p);
                })
            })
            .await
            .map_err(|e| AppError::Other(e.to_string()))??;
        }
        Kind::Config => unreachable!(),
    }

    // 2. 收尾：环境变量
    emit_state(&app, &id, Phase::Finalizing, crate::tr!("配置环境变量", "Configuring environment variables"), Some(0.95));
    finalize_env(&app, &item, &params)?;

    // 3. 验证
    emit_state(&app, &id, Phase::Verifying, crate::tr!("验证中", "Verifying"), Some(0.98));
    let item2 = item.clone();
    let params2 = params.clone();
    let out = tauri::async_runtime::spawn_blocking(move || verify::run_verify(&item2, &params2, &[]))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    if out.ok {
        log(&app, "info", Some(&id), crate::tr!("验证通过：{0}", "Verification passed: {0}", out.output.lines().next().unwrap_or("")));
        emit_state(&app, &id, Phase::Done, crate::tr!("已完成", "Completed"), Some(1.0));
        Ok(())
    } else {
        log(&app, "warn", Some(&id), crate::tr!("验证未通过（退出码 {0}）：{1}", "Verification failed (exit code {0}): {1}", out.code, out.output));
        emit_state(&app, &id, Phase::Failed, crate::tr!("验证未通过，可手动核对", "Verification failed; check manually"), Some(1.0));
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
            env::PathWrite::Skipped => log(app, "info", Some(&item.id), crate::tr!("PATH 已由安装器配置，跳过：{0}", "PATH already configured by installer, skipping: {0}", d)),
            env::PathWrite::Appended => log(app, "info", Some(&item.id), crate::tr!("PATH 已追加：{0}", "PATH appended: {0}", d)),
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
    let item = manifest.get(&id).cloned().ok_or_else(|| AppError::Other(crate::tr!("未知配置项 {0}", "Unknown config item {0}", id)))?;
    if item.kind != Kind::Config {
        return Err(AppError::Other(crate::tr!("{0} 不是配置项", "{0} is not a config item", id)));
    }

    // 依赖检查
    for dep in &item.requires {
        let dep_item = manifest.get(dep).ok_or_else(|| AppError::Other(crate::tr!("缺少依赖项 {0}", "Missing dependency {0}", dep)))?;
        let r = detect::detect_item(dep_item);
        if !matches!(r.status, detect::DetectStatus::Installed | detect::DetectStatus::Configured) {
            let msg = crate::tr!("需先安装 {0}", "Please install {0} first", dep_item.display_name());
            emit_state(&app, &id, Phase::Failed, &msg, None);
            return Err(AppError::Config(msg));
        }
    }

    emit_state(&app, &id, Phase::NotConfigured, crate::tr!("备份并执行", "Backing up and running"), Some(0.2));
    let path = env::backup_env(&crate::tr!("{0}（配置前）", "{0} (before configure)", item.display_name()))?;
    log(&app, "info", Some(&id), crate::tr!("已备份环境变量：{0}", "Environment variables backed up: {0}", path.display()));

    let item2 = item.clone();
    let params2 = params.clone();
    emit_finalizing(&app, &id, &crate::tr!("开始执行配置动作", "Running configuration actions"), 0.4);
    let logs = tauri::async_runtime::spawn_blocking(move || config::run_actions(&item2, mirror_url.as_deref(), &params2))
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    for l in &logs {
        log(&app, if l.ok { "info" } else { "warn" }, Some(&id), l.message.clone());
    }

    emit_state(&app, &id, Phase::Verifying, crate::tr!("复查中", "Re-checking"), Some(0.9));
    let item3 = item.clone();
    let out = tauri::async_runtime::spawn_blocking(move || detect::detect_item(&item3))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    if out.status == detect::DetectStatus::Configured {
        emit_state(&app, &id, Phase::Configured, crate::tr!("已配置", "Configured"), Some(1.0));
    } else {
        emit_state(&app, &id, Phase::NotConfigured, crate::tr!("执行完成，但复查未确认", "Completed, but re-check did not confirm"), Some(1.0));
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
