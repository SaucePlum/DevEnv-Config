//! 检测每项的已装 / 已配置状态。设计依据：设计文档第 8 节 detect 规则、第 18 节。
//! 优先级：command → registry_display_name_prefix → path → file_contains → env

use crate::manifest::{Detect, Item, Mirror, Scope};
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
    /// 换源类条目的当前值（检测命令回显的源地址），供界面把下拉框回填成真实状态。
    pub mirror_current: Option<String>,
}

impl DetectResult {
    fn installed(version: Option<String>, detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::Installed, version, detail: Some(detail.into()), mirror_current: None }
    }
    fn not_installed(detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::NotInstalled, version: None, detail: Some(detail.into()), mirror_current: None }
    }
    fn configured(detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::Configured, version: None, detail: Some(detail.into()), mirror_current: None }
    }
    fn not_configured(detail: impl Into<String>) -> Self {
        Self { status: DetectStatus::NotConfigured, version: None, detail: Some(detail.into()), mirror_current: None }
    }
}

/// 在文本里找出清单中当前生效的镜像：取出现位置最靠前的那个（重复写入时最新的通常在最前），
/// 位置相同时取更长的地址，避免短地址被更长的地址包含时误判。
fn find_mirror<'a>(text: &str, mirrors: &'a [Mirror]) -> Option<&'a Mirror> {
    let lower = text.to_ascii_lowercase();
    mirrors
        .iter()
        .filter_map(|m| {
            let url = m.url.trim_end_matches('/').to_ascii_lowercase();
            if url.is_empty() {
                return None;
            }
            lower.find(&url).map(|at| (at, std::cmp::Reverse(m.url.len()), m))
        })
        .min_by_key(|(at, len, _)| (*at, *len))
        .map(|(_, _, m)| m)
}

/// 取文本里第一行非空内容：命令回显或环境变量里存的就是当前源地址。
fn first_line(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// 换源类条目的检测结果：文本里出现清单中的镜像才算「已换源」，并把该地址回传界面回填下拉框。
/// 没命中时算「未换源」，`raw` 是原始回显值（界面据此显示当前用的是非镜像源）。
fn mirror_detect(text: &str, raw: Option<String>, mirrors: &[Mirror]) -> DetectResult {
    match find_mirror(text, mirrors) {
        Some(m) => {
            let mut r = DetectResult::configured(m.url.clone());
            r.mirror_current = Some(m.url.clone());
            r
        }
        None => {
            let raw = raw.filter(|s| !s.trim().is_empty());
            let mut r = DetectResult::not_configured(
                raw.clone().unwrap_or_else(|| crate::tr!("未检测到当前源地址", "Could not read the current source")),
            );
            r.mirror_current = raw;
            r
        }
    }
}

/// 读环境变量：用户级注册表优先（应用写入的就是它，进程环境可能是旧值），进程环境兜底。
fn env_value(name: &str) -> Option<String> {
    crate::env::read_var(Scope::User, name)
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var(name).ok().filter(|v| !v.trim().is_empty()))
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
                    let detail = out.combined();
                    if item.kind == crate::manifest::Kind::Config {
                        // 换源类条目：把回显的当前值同镜像清单比对。只有确实指向清单里的
                        // 某个镜像才算「已换源」——官方源（yarn 默认的 registry.yarnpkg.com、
                        // npm 默认的 registry.npmjs.org）不该被当成已配置。
                        if !item.mirrors.is_empty() {
                            return mirror_detect(&detail, Some(first_line(&detail)), &item.mirrors);
                        }
                        return DetectResult::configured(detail);
                    }
                    return DetectResult::installed(v, crate::tr!("检测通过：{0}", "Detected: {0}", detail.lines().next().unwrap_or("命令可用")));
                }
                Ok(out) => {
                    let detail = crate::tr!("检测命令运行异常（退出码 {0}）：{1}", "Detection command failed (exit code {0}): {1}", out.code, out.combined());
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
            Ok(Some(ver)) => return DetectResult::installed(Some(ver.clone()), crate::tr!("已安装（注册表版本 {0}）", "Installed (registry version {0})", ver)),
            Ok(None) => registry_miss = Some(DetectResult::not_installed(crate::tr!("注册表中没有「{0}」的安装记录", "No install record for \"{0}\" found in the registry", prefix))),
            Err(e) => registry_miss = Some(DetectResult::not_installed(e)),
        }
    }

    if let Some(p) = &d.path {
        let path = PathBuf::from(crate::env_norm::expand_env(p));
        let is_config = item.kind == crate::manifest::Kind::Config;
        let mut res = if path.exists() {
            DetectResult::installed(None, crate::tr!("已找到 {0}", "Found {0}", path.display()))
        } else {
            let mut detail = crate::tr!("未找到 {0}", "Not found: {0}", path.display());
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
                            let sep = if crate::i18n::is_en() { ", " } else { "、" };
                            detail.push_str(&crate::tr!(
                                "；已存在其他公钥（{0}），可生成新的 ed25519 并存",
                                "; other public keys already exist ({0}); a new ed25519 key can coexist",
                                others.join(sep)
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
                Ok(text) => {
                    // 换源类条目：配置文件里写着清单中的镜像才算「已换源」（应用写入的就是该地址）
                    if !item.mirrors.is_empty() {
                        if let Some(m) = find_mirror(&text, &item.mirrors) {
                            let mut r = DetectResult::configured(m.url.clone());
                            r.mirror_current = Some(m.url.clone());
                            return r;
                        }
                    }
                    if text.contains(needle) {
                        DetectResult::configured(crate::tr!("配置已生效（包含 {0}）", "Config active (contains {0})", needle))
                    } else {
                        DetectResult::not_configured(crate::tr!("配置文件中还没有 {0}", "Config file does not yet contain {0}", needle))
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    DetectResult::not_configured(crate::tr!("配置文件尚未生成（执行配置后自动创建）", "Config file not generated yet (created automatically after configuration)"))
                }
                Err(_) => DetectResult::not_configured(crate::tr!("配置文件无法读取: {0}", "Cannot read config file: {0}", path.display())),
            };
        }
    }

    if let Some(names) = &d.env {
        let vals: Vec<String> = names.iter().map(|n| env_value(n).unwrap_or_default()).collect();
        if !vals.iter().all(|v| !v.trim().is_empty()) {
            return DetectResult::not_configured(crate::tr!("相关环境变量尚未设置", "Related environment variables are not set yet"));
        }
        // 换源类条目：环境变量里写着清单中的镜像才算「已换源」
        if !item.mirrors.is_empty() {
            let text = vals.join("\n");
            return mirror_detect(&text, Some(first_line(&text)), &item.mirrors);
        }
        return DetectResult::configured(crate::tr!("相关环境变量已设置", "Related environment variables are set"));
    }

    if let Some(miss) = registry_miss {
        return miss;
    }
    if item.kind == crate::manifest::Kind::Config {
        DetectResult::not_configured(crate::tr!("暂无适用的检测规则", "No applicable detection rule"))
    } else {
        DetectResult::not_installed(crate::tr!("暂无适用的检测规则", "No applicable detection rule"))
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

    #[test]
    fn mirror_item_counts_as_configured_only_for_a_known_mirror() {
        let mut it = item("npm-mirror");
        // 回显指向清单里的镜像即算已换源（结尾斜杠差异应被容忍）
        for url in ["https://registry.npmmirror.com", "https://registry.npmmirror.com/"] {
            it.detect = Some(Detect {
                command: Some(vec!["cmd".into(), "/C".into(), format!("echo {url}")]),
                ..Default::default()
            });
            let r = detect_item(&it);
            assert_eq!(r.status, DetectStatus::Configured, "{url} 应视为已换源");
            assert_eq!(r.mirror_current.as_deref(), Some("https://registry.npmmirror.com"));
        }
        // 官方源不在清单里 → 未换源（此前任何回显都被当成已配置，属误报）
        it.detect = Some(Detect {
            command: Some(vec!["cmd".into(), "/C".into(), "echo https://registry.npmjs.org".into()]),
            ..Default::default()
        });
        let r = detect_item(&it);
        assert_eq!(r.status, DetectStatus::NotConfigured);
        assert_eq!(r.mirror_current.as_deref(), Some("https://registry.npmjs.org"));
    }

    #[test]
    fn find_mirror_ignores_case_and_trailing_slash() {
        let mirrors = item("npm-mirror").mirrors;
        assert_eq!(
            find_mirror("https://Registry.NPMMIRROR.com/react/-/react-19.3.0.tgz", &mirrors).map(|m| m.url.as_str()),
            Some("https://registry.npmmirror.com")
        );
        assert!(find_mirror("https://registry.npmjs.org", &mirrors).is_none());
        assert!(find_mirror("", &mirrors).is_none());
    }

    #[test]
    fn env_mirror_item_returns_current_value_for_backfill() {
        // Rust 换源：地址存在环境变量里，检测要把清单内的镜像回传，界面才能回填下拉框
        let mut it = item("rust-mirror");
        let aliyun = it.mirrors.iter().find(|m| m.url.contains("aliyun")).unwrap().url.clone();
        it.detect = Some(Detect { env: Some(vec!["DEVENV_PROBE_RUST_DIST".into()]), ..Default::default() });

        std::env::set_var("DEVENV_PROBE_RUST_DIST", &aliyun);
        let r = detect_item(&it);
        assert_eq!(r.status, DetectStatus::Configured);
        assert_eq!(r.mirror_current.as_deref(), Some(aliyun.as_str()));

        // 指向清单外的地址（如官方源）时算未换源，但仍回传真实值
        std::env::set_var("DEVENV_PROBE_RUST_DIST", "https://static.rust-lang.org");
        let r = detect_item(&it);
        assert_eq!(r.status, DetectStatus::NotConfigured);
        assert_eq!(r.mirror_current.as_deref(), Some("https://static.rust-lang.org"));

        std::env::remove_var("DEVENV_PROBE_RUST_DIST");
        assert_eq!(detect_item(&it).status, DetectStatus::NotConfigured, "变量缺失时仍应是未配置");
    }

    #[test]
    fn file_mirror_item_returns_current_value_for_backfill() {
        // Cargo 换源：地址写在配置文件里，检测要能从中读出清单内的镜像
        let dir = std::env::temp_dir().join("devenv-mirror-probe");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        let mirrors = item("cargo-mirror").mirrors;
        let ustc = mirrors.iter().find(|m| m.url.contains("ustc")).unwrap().url.clone();

        let mut it = item("cargo-mirror");
        it.detect = Some(Detect {
            file_contains: Some(vec![file.to_string_lossy().to_string(), "replace-with".into()]),
            ..Default::default()
        });

        std::fs::write(&file, format!("[source.crates-io]\nreplace-with = \"mirror\"\n[source.mirror]\nregistry = \"sparse+{ustc}\"\n")).unwrap();
        let r = detect_item(&it);
        assert_eq!(r.status, DetectStatus::Configured);
        assert_eq!(r.mirror_current.as_deref(), Some(ustc.as_str()));

        // 命中关键字但没有清单内的镜像地址时，沿用原有判定且不回传值
        std::fs::write(&file, "[source.crates-io]\nreplace-with = \"mirror\"\n").unwrap();
        let r = detect_item(&it);
        assert_eq!(r.status, DetectStatus::Configured);
        assert_eq!(r.mirror_current, None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
