//! 检测每项的已装 / 已配置状态。设计依据：设计文档第 8 节 detect 规则、第 18 节。
//! 优先级：command → registry_display_name_prefix → path → file_contains → env

use crate::manifest::{Detect, Item, Scope};
use crate::proc;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectStatus {
    Installed,
    NotInstalled,
    Configured,
    NotConfigured,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectResult {
    pub status: DetectStatus,
    pub version: Option<String>,
    pub detail: Option<String>,
}

impl DetectResult {
    fn installed(version: Option<String>, detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::Installed, version, detail: Some(detail.into()) }
    }
    fn not_installed(detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::NotInstalled, version: None, detail: Some(detail.into()) }
    }
    fn configured(detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::Configured, version: None, detail: Some(detail.into()) }
    }
    fn not_configured(detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::NotConfigured, version: None, detail: Some(detail.into()) }
    }
}

/// 从项自身的 env.path_append 里取出可用的绝对目录，作为「PATH 未刷新」的兜底查找位置。
/// `%VAR%` 形式的条目（如 Miniconda 的 `%USERPROFILE%\miniconda3`）先展开再入列，
/// 否则字面量路径永远匹配不到文件，兜底失效。
pub fn extra_dirs(item: &Item) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(env) = &item.env {
        for p in &env.path_append {
            if !p.contains('{') {
                out.push(PathBuf::from(crate::env_norm::expand_env(p)));
            }
        }
    }
    out
}

pub fn detect_item(item: &Item) -> DetectResult {
    let d = match &item.detect {
        Some(d) => d.clone(),
        None => Detect::default(),
    };
    let timeout = Duration::from_secs(d.timeout_secs.unwrap_or(15));

    if let Some(cmd) = &d.command {
        if let Some((program, args)) = cmd.split_first() {
            match proc::run_program(program, args, &extra_dirs(item), timeout) {
                Ok(out) if out.code == 0 => {
                    let v = out.first_version();
                    let is_config = item.kind == crate::manifest::Kind::Config;
                    let detail = out.combined();
                    return if is_config {
                        DetectResult::configured(detail)
                    } else {
                        DetectResult::installed(v, format!("检测通过：{}", detail.lines().next().unwrap_or("命令可用")))
                    };
                }
                Ok(out) => {
                    let detail = format!("检测命令运行异常（退出码 {}）：{}", out.code, out.combined());
                    return if item.kind == crate::manifest::Kind::Config {
                        DetectResult::not_configured(detail)
                    } else {
                        DetectResult::not_installed(detail)
                    };
                }
                Err(e) => {
                    return if item.kind == crate::manifest::Kind::Config {
                        DetectResult::not_configured(e.to_string())
                    } else {
                        DetectResult::not_installed(e.to_string())
                    }
                }
            }
        }
    }

    // 注册表未命中时不立即判负：允许后续规则兜底
    // （例：VC++ 运行库——新版 redist 注册名已从"2015-2022 Redistributable"改为"v14 Redistributable"，
    //   而 VS 自带的运行库注册为"2022 X64 Minimum/Additional Runtime"；只要 System32 的 DLL 在，应用即可运行）
    let mut registry_miss: Option<DetectResult> = None;
    if let Some(prefix) = &d.registry_display_name_prefix {
        match registry_lookup(prefix) {
            Ok(Some(ver)) => return DetectResult::installed(Some(ver.clone()), format!("已安装（注册表版本 {ver}）")),
            Ok(None) => registry_miss = Some(DetectResult::not_installed(format!("注册表中没有「{prefix}」的安装记录"))),
            Err(e) => registry_miss = Some(DetectResult::not_installed(e)),
        }
    }

    if let Some(p) = &d.path {
        let path = PathBuf::from(crate::env_norm::expand_env(p));
        let is_config = item.kind == crate::manifest::Kind::Config;
        let mut res = if path.exists() {
            DetectResult::installed(None, format!("已找到 {}", path.display()))
        } else {
            let mut detail = format!("未找到 {}", path.display());
            // SSH 场景：期望密钥缺失时，列出已有哪些公钥，说明"并存"策略
            if path.extension().map(|e| e == "pub").unwrap_or(false) {
                if let Some(dir) = path.parent() {
                    if let Ok(rd) = std::fs::read_dir(dir) {
                        let others: Vec<String> = rd
                            .flatten()
                            .filter(|e| e.path().extension().map(|x| x == "pub").unwrap_or(false))
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect();
                        if !others.is_empty() {
                            detail.push_str(&format!(
                                "；已存在其他公钥（{}），可生成新的 ed25519 并存",
                                others.join("、")
                            ));
                        }
                    }
                }
            }
            DetectResult::not_installed(detail)
        };
        // 配置类条目（如 ssh-key 检测 ~/.ssh 目录）语义是「已配置/未配置」，
        // 前端配置页只认 configured，否则即使检测成功也显示未配置
        if is_config {
            res.status = match res.status {
                DetectStatus::Installed => DetectStatus::Configured,
                DetectStatus::NotInstalled => DetectStatus::NotConfigured,
                other => other,
            };
        }
        return res;
    }

    if let Some(fc) = &d.file_contains {
        if fc.len() == 2 {
            let path = PathBuf::from(crate::env_norm::expand_env(&fc[0]));
            let needle = &fc[1];
            return match std::fs::read_to_string(&path) {
                Ok(text) if text.contains(needle) => DetectResult::configured(format!("配置已生效（包含 {needle}）")),
                Ok(_) => DetectResult::not_configured(format!("配置文件中还没有 {needle}")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    DetectResult::not_configured("配置文件尚未生成（执行配置后自动创建）")
                }
                Err(_) => DetectResult::not_configured(format!("配置文件无法读取: {}", path.display())),
            };
        }
    }

    if let Some(names) = &d.env {
        let all = names.iter().all(|n| {
            std::env::var(n).map(|v| !v.trim().is_empty()).unwrap_or(false)
                || crate::env::read_var(Scope::User, n).map(|v| !v.trim().is_empty()).unwrap_or(false)
        });
        return if all {
            DetectResult::configured("相关环境变量已设置")
        } else {
            DetectResult::not_configured("相关环境变量尚未设置")
        };
    }

    if let Some(miss) = registry_miss {
        return miss;
    }
    if item.kind == crate::manifest::Kind::Config {
        DetectResult::not_configured("暂无适用的检测规则")
    } else {
        DetectResult::not_installed("暂无适用的检测规则")
    }
}

/// 在 HKLM（原生 + WOW6432Node）与 HKCU（原生）的卸载项里按 DisplayName 前缀查找，返回 DisplayVersion。
fn registry_lookup(prefix: &str) -> Result<Option<String>, String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    const NATIVE: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const WOW64: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    let roots: [(&RegKey, &str); 3] = [(&hklm, NATIVE), (&hklm, WOW64), (&hkcu, NATIVE)];

    for (root, sub) in roots {
        let key = match root.open_subkey_with_flags(sub, KEY_READ) {
            Ok(k) => k,
            Err(_) => continue,
        };
        for sub_name in key.enum_keys().flatten() {
            let subkey = match key.open_subkey_with_flags(&sub_name, KEY_READ) {
                Ok(k) => k,
                Err(_) => continue,
            };
            let name: String = match subkey.get_value("DisplayName") {
                Ok(v) => v,
                Err(_) => continue,
            };
            if name.starts_with(prefix) {
                let ver: String = subkey.get_value("DisplayVersion").unwrap_or_default();
                return Ok(Some(if ver.is_empty() { name } else { ver }));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Kind, Manifest, Source};

    fn item(id: &str) -> Item {
        Manifest::load().unwrap().get(id).unwrap().clone()
    }

    #[test]
    fn cmd_detection_runs_and_parses_version() {
        let mut it = item("git");
        it.detect = Some(Detect {
            command: Some(vec!["cmd".into(), "/C".into(), "echo 9.9.9".into()]),
            ..Default::default()
        });
        let r = detect_item(&it);
        assert_eq!(r.status, DetectStatus::Installed);
        assert_eq!(r.version.as_deref(), Some("9.9.9"));
    }

    #[test]
    fn missing_command_is_reported_as_not_installed() {
        let mut it = item("git");
        it.detect = Some(Detect {
            command: Some(vec!["no_such_program_xyz".into(), "--version".into()]),
            ..Default::default()
        });
        assert_eq!(detect_item(&it).status, DetectStatus::NotInstalled);
    }

    #[test]
    fn config_item_uses_configured_status() {
        let mut it = item("git-identity");
        it.detect = Some(Detect {
            command: Some(vec!["cmd".into(), "/C".into(), "echo ok".into()]),
            ..Default::default()
        });
        assert_eq!(detect_item(&it).status, DetectStatus::Configured);
    }

    #[test]
    fn path_detection_reports_missing_file() {
        let mut it = item("vs2022");
        it.detect = Some(Detect {
            path: Some("C:\\definitely\\not\\here\\devenv.exe".into()),
            ..Default::default()
        });
        assert_eq!(detect_item(&it).status, DetectStatus::NotInstalled);
    }

    #[test]
    fn extra_dirs_excludes_placeholder_paths() {
        let it = item("redis"); // env.path_append 含 {install_dir}
        assert!(extra_dirs(&it).is_empty());
        let it2 = item("cmake"); // 绝对路径
        assert_eq!(extra_dirs(&it2).len(), 1);
    }

    #[test]
    fn extra_dirs_expands_percent_vars() {
        // Miniconda 场景：%USERPROFILE% 必须展开成真实目录，否则兜底查找永远失败
        let it = item("miniconda");
        let dirs = extra_dirs(&it);
        assert!(!dirs.is_empty());
        assert!(
            dirs.iter().all(|d| !d.to_string_lossy().contains('%')),
            "%VAR% 应已展开，实际 {dirs:?}"
        );
    }

    #[test]
    fn registry_miss_falls_through_to_path_rule() {
        // 模拟 VC++ 场景：注册表命名变了匹配不上，但 System32 运行库 DLL 在 → 应视为已安装
        let mut it = item("vcredist");
        it.detect = Some(Detect {
            registry_display_name_prefix: Some("NoSuchProduct_XYZ_123".into()),
            path: Some("C:\\Windows\\System32\\kernel32.dll".into()),
            ..Default::default()
        });
        assert_eq!(detect_item(&it).status, DetectStatus::Installed);
    }

    #[test]
    fn registry_only_miss_still_reports_not_installed() {
        let mut it = item("mariadb");
        it.detect = Some(Detect {
            registry_display_name_prefix: Some("NoSuchProduct_XYZ_123".into()),
            ..Default::default()
        });
        let r = detect_item(&it);
        assert_eq!(r.status, DetectStatus::NotInstalled);
        assert!(r.detail.unwrap_or_default().contains("NoSuchProduct_XYZ_123"));
    }

    #[test]
    fn path_detection_accepts_existing_directory() {
        // ssh-key 的检测目标是 ~/.ssh 目录本身：目录存在即视为已配置
        let mut it = item("vs2022");
        it.detect = Some(Detect {
            path: Some(std::env::temp_dir().to_string_lossy().to_string()),
            ..Default::default()
        });
        assert_eq!(detect_item(&it).status, DetectStatus::Installed);
    }

    #[test]
    fn config_item_path_detect_maps_to_configured() {
        // 配置类条目（ssh-key）的 path 检测命中时应返回 Configured 而非 Installed，
        // 否则前端配置页会显示「未配置」
        let mut it = item("ssh-key");
        assert_eq!(it.kind, crate::manifest::Kind::Config, "清单里 ssh-key 应为 config 类型");
        it.detect = Some(Detect {
            path: Some(std::env::temp_dir().to_string_lossy().to_string()),
            ..Default::default()
        });
        assert_eq!(detect_item(&it).status, DetectStatus::Configured);
    }

    #[test]
    fn manifest_kind_smoke() {
        let m = Manifest::load().unwrap();
        assert!(m.items.iter().any(|i| i.kind == Kind::Config && i.source == Source::Config));
    }

    #[test]
    fn env_detection_reports_not_configured_for_missing_var() {
        let mut it = item("rust-mirror");
        it.detect = Some(Detect { env: Some(vec!["NO_SUCH_ENV_XYZ_123".into()]), ..Default::default() });
        assert_eq!(detect_item(&it).status, DetectStatus::NotConfigured);
    }
}
