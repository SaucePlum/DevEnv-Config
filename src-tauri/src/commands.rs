//! 前端可调用的命令。所有返回 Result<T, AppError>。

use crate::detect::{self, DetectStatus};
use crate::error::AppError;
use crate::installer;
use crate::manifest::{Item, Manifest, Mirror};
use crate::provider::{self, VersionInfo};
use crate::state::{self, Params};
use crate::{config, download, env};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, State};

pub struct AppState {
    pub manifest: Manifest,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    pub id: String,
    pub name: String,
    pub category: String,
    pub source: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub requires: Vec<String>,
    pub mirrors: Vec<Mirror>,
    pub status: DetectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 换源类条目的当前值（源地址），前端据此把镜像下拉框回填成真实状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_current: Option<String>,
    pub needs_dir: bool,
    pub has_verify: bool,
    pub archive: Option<String>,
}

fn view(item: &Item) -> ItemView {
    let r = detect::detect_item(item);
    ItemView {
        id: item.id.clone(),
        name: item.display_name(),
        category: item.category.clone(),
        source: match item.source {
            crate::manifest::Source::Online => "online",
            crate::manifest::Source::Local => "local",
            crate::manifest::Source::Config => "config",
        }
        .into(),
        kind: match item.kind {
            crate::manifest::Kind::Installer => "installer",
            crate::manifest::Kind::Portable => "portable",
            crate::manifest::Kind::Config => "config",
        }
        .into(),
        note: item.display_note(),
        requires: item.requires.clone(),
        // 镜像源名按当前语言返回，前端直接展示 name 即可
        mirrors: item
            .mirrors
            .iter()
            .map(|m| Mirror { name: m.display_name(), name_en: None, url: m.url.clone() })
            .collect(),
        status: r.status,
        version: r.version,
        detail: r.detail,
        mirror_current: r.mirror_current,
        needs_dir: item.needs_install_dir(),
        has_verify: !item.verify.is_empty(),
        archive: item.archive.map(|a| match a {
            crate::manifest::Archive::Zip => "zip".into(),
            crate::manifest::Archive::SevenZ => "7z".into(),
        }),
    }
}

#[tauri::command]
pub async fn list_items(state: State<'_, AppState>) -> Result<Vec<ItemView>, AppError> {
    let items = state.manifest.items.clone();
    tauri::async_runtime::spawn_blocking(move || items.iter().map(view).collect())
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_versions(state: State<'_, AppState>, id: String) -> Result<Vec<VersionInfo>, AppError> {
    let item = state.manifest.get(&id).cloned().ok_or_else(|| AppError::Other(crate::tr!("未知条目 {0}", "Unknown item {0}", id)))?;
    provider::list_versions(&item).await.map_err(|e| AppError::Provider(e.to_string()))
}

#[tauri::command]
pub async fn start_item(app: AppHandle, state: State<'_, AppState>, id: String, version: Option<String>, params: Option<Params>) -> Result<(), AppError> {
    let manifest = state.manifest.clone();
    state::start_item(app, manifest, id, version, params.unwrap_or_default()).await
}

#[tauri::command]
pub async fn run_config(app: AppHandle, state: State<'_, AppState>, id: String, mirror_url: Option<String>, params: Option<Params>) -> Result<Vec<config::StepLog>, AppError> {
    let manifest = state.manifest.clone();
    state::run_config(app, manifest, id, mirror_url, params.unwrap_or_default()).await
}

#[tauri::command]
pub fn is_elevated() -> bool {
    installer::is_elevated()
}

/// 读取某配置条目的表单默认值。同步子进程调用被移到阻塞线程，避免卡住主线程。
#[tauri::command]
pub async fn config_defaults(state: State<'_, AppState>, id: String) -> Result<HashMap<String, String>, AppError> {
    state.manifest.get(&id).ok_or_else(|| AppError::Other(crate::tr!("未知条目 {0}", "Unknown item {0}", id)))?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = HashMap::new();
        if id == "git-identity" || id == "ssh-key" {
            if let Ok(o) = crate::proc::run_program("git", &["config".into(), "--global".into(), "user.name".into()], &[], std::time::Duration::from_secs(10)) {
                if o.code == 0 {
                    out.insert("git_name".into(), o.combined());
                }
            }
            if let Ok(o) = crate::proc::run_program("git", &["config".into(), "--global".into(), "user.email".into()], &[], std::time::Duration::from_secs(10)) {
                if o.code == 0 {
                    out.insert("git_email".into(), o.combined());
                }
            }
        }
        if id == "npm-dirs" {
            // 回填 npm 当前的 prefix / cache，供弹窗直接显示已有配置
            // 注意：未设置时 npm 会输出字面量 "undefined"，需过滤
            for (key, field) in [("prefix_dir", "prefix"), ("cache_dir", "cache")] {
                if let Ok(o) = crate::proc::run_program(
                    "npm",
                    &["config".into(), "get".into(), field.into()],
                    &[],
                    std::time::Duration::from_secs(10),
                ) {
                    if o.code == 0 {
                        let v = o.combined().trim().to_string();
                        if !v.is_empty() && v != "undefined" {
                            out.insert(key.into(), v);
                        }
                    }
                }
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// 免安装类投放前的目录检查：返回 true 表示目录已存在且非空，需要用户确认覆盖。
#[tauri::command]
pub fn dir_needs_confirm(dir: String) -> bool {
    crate::deploy::dir_non_empty(&PathBuf::from(dir))
}

/// 列出 ~/.ssh 下所有 .pub 公钥的完整路径（供界面下拉选择查看）。
#[tauri::command]
pub fn list_ssh_public_keys() -> Result<Vec<String>, AppError> {
    let dir = PathBuf::from(crate::env_norm::expand_env("%USERPROFILE%\\.ssh"));
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.to_lowercase().ends_with(".pub") {
                out.push(e.path().to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct SshKeyStatus {
    pub has_public: bool,
    pub has_private: bool,
}

/// ed25519 密钥状态：供界面按状态显示按钮文案（生成新密钥 / 重建公钥 / 复制公钥）。
#[tauri::command]
pub fn ssh_key_status() -> SshKeyStatus {
    let dir = PathBuf::from(crate::env_norm::expand_env("%USERPROFILE%\\.ssh"));
    SshKeyStatus {
        has_public: dir.join("id_ed25519.pub").is_file(),
        has_private: dir.join("id_ed25519").is_file(),
    }
}

/// 读取公钥内容。`file` 为 ~/.ssh 下的文件名（须以 .pub 结尾），缺省为 id_ed25519.pub。
#[tauri::command]
pub fn read_public_key(file: Option<String>) -> Result<String, AppError> {
    let name = match file.as_deref() {
        None | Some("") => "id_ed25519.pub".to_string(),
        Some(f) => {
            if f.contains('\\') || f.contains('/') || f.contains("..") || !f.to_lowercase().ends_with(".pub") {
                return Err(AppError::Config(crate::tr!("非法的密钥文件名", "Invalid key file name").into()));
            }
            f.to_string()
        }
    };
    let path = PathBuf::from(crate::env_norm::expand_env("%USERPROFILE%\\.ssh")).join(&name);
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|_| AppError::Config(crate::tr!("公钥不存在：{0}", "Public key not found: {0}", path.display())))
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), AppError> {
    let mut cb = arboard::Clipboard::new().map_err(|e| AppError::Config(e.to_string()))?;
    cb.set_text(text).map_err(|e| AppError::Config(e.to_string()))
}

/// 用系统默认浏览器打开地址（https 白名单校验在 `installer::open_externally` 内）。
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || installer::open_externally(&url))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}

/// 测试 GitHub SSH 认证连通性，返回 ssh 的原始输出。
/// 注意：GitHub 对成功认证也返回退出码 1（does not provide shell access），
/// 前端应按输出内容而非退出码判定成功与否。
#[tauri::command]
pub async fn test_ssh() -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(|| {
        let ssh = crate::proc::which("ssh", &[PathBuf::from("C:\\Windows\\System32\\OpenSSH")])
            .ok_or_else(|| AppError::Config(crate::tr!("找不到 ssh.exe，请先在「可选功能」中安装 OpenSSH 客户端", "ssh.exe not found; install the OpenSSH Client from Windows optional features").into()))?;
        let out = crate::proc::run_capture(
            &ssh,
            &[
                "-o".into(),
                "StrictHostKeyChecking=accept-new".into(),
                "-T".into(),
                "git@github.com".into(),
            ],
            std::time::Duration::from_secs(30),
        )?;
        Ok(crate::tr!("退出码 {0}\n{1}", "Exit code {0}\n{1}", out.code, out.combined()))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub fn list_env_backups() -> Result<Vec<String>, AppError> {
    env::list_backups()
}

#[tauri::command]
pub fn read_env_backup(file: String) -> Result<String, AppError> {
    env::read_backup(&file)
}

#[tauri::command]
pub fn delete_env_backup(file: String) -> Result<(), AppError> {
    env::delete_backup(&file)
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheEntry {
    pub name: String,
    pub size: u64,
    pub modified: String,
}

/// 列出下载缓存中的文件（名称、字节、修改时间），按时间倒序。
#[tauri::command]
pub fn list_cache_entries() -> Result<Vec<CacheEntry>, AppError> {
    let dir = download::cache_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let m = match e.metadata() {
                Ok(m) if m.is_file() => m,
                _ => continue,
            };
            let modified = m
                .modified()
                .ok()
                .and_then(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string().into())
                .unwrap_or_default();
            out.push(CacheEntry { name: e.file_name().to_string_lossy().to_string(), size: m.len(), modified });
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

/// 删除单个缓存文件。
#[tauri::command]
pub fn delete_cache_entry(name: String) -> Result<(), AppError> {
    if name.contains('\\') || name.contains('/') || name.contains("..") {
        return Err(AppError::Config(crate::tr!("非法的文件名", "Invalid file name").into()));
    }
    let p = download::cache_dir().join(name);
    std::fs::remove_file(&p).map_err(|e| AppError::Config(crate::tr!("删除失败: {0}", "Delete failed: {0}", e)))
}

/// 清空全部下载缓存，返回删除的文件数。
#[tauri::command]
pub fn clear_cache() -> Result<u32, AppError> {
    let dir = download::cache_dir();
    let mut n = 0u32;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path().is_file() && std::fs::remove_file(e.path()).is_ok() {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// 在资源管理器中打开下载缓存目录（路径固定，无外部输入）。
#[tauri::command]
pub fn open_cache_dir() -> Result<(), AppError> {
    let dir = download::cache_dir();
    std::fs::create_dir_all(&dir)?;
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| AppError::Config(crate::tr!("打开目录失败: {0}", "Failed to open directory: {0}", e)))?;
    Ok(())
}

/// 当前应用版本（与关于页、安装包版本一致）。
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 切换界面语言（"zh" / "en"）。之后由后端生成的文案（软件名、备注、进度、日志、错误）随之切换。
#[tauri::command]
pub fn set_language(lang: String) {
    crate::i18n::set_lang(&lang);
}

#[tauri::command]
pub fn restore_env_backup(file: String) -> Result<(), AppError> {
    env::restore_backup(&file)
}

#[tauri::command]
pub fn cache_size_mb() -> f64 {
    fn dir_size(p: &std::path::Path) -> u64 {
        let mut total = 0;
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                if let Ok(m) = e.metadata() {
                    if m.is_file() {
                        total += m.len();
                    } else if m.is_dir() {
                        total += dir_size(&e.path());
                    }
                }
            }
        }
        total
    }
    dir_size(&download::cache_dir()) as f64 / 1024.0 / 1024.0
}
