//! 配置项动作执行：env_set / toml_merge / command / ssh_keygen。
//! 设计依据：设计文档第 8 节动作类型表、第 14 节。

use crate::env;
use crate::env_norm;
use crate::error::{AppError, AppResult};
use crate::manifest::{Action, Item};
use crate::proc;
use crate::state::Params;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize)]
pub struct StepLog {
    pub ok: bool,
    pub message: String,
}

fn extra_dirs(item: &Item) -> Vec<PathBuf> {
    crate::detect::extra_dirs(item)
}

/// 执行一项配置的所有动作，逐条返回日志。任一非 optional 动作失败即中止。
pub fn run_actions(item: &Item, mirror_url: Option<&str>, params: &Params) -> AppResult<Vec<StepLog>> {
    let mut logs = Vec::new();
    let fill_raw = |s: &str| -> String {
        let mut v = params.fill(s);
        if let Some(m) = mirror_url {
            v = v.replace("{mirror_url}", m);
        }
        v
    };
    let placeholder_re = regex::Regex::new(r"\{[a-z_]+\}").ok();
    // 残留占位符检查：缺参数直接报错，避免静默写出含 {xxx} 的垃圾配置。
    // 此时只是算字符串，尚未执行任何动作或备份，安全。
    let fill = |s: &str| -> AppResult<String> {
        let v = fill_raw(s);
        if let Some(rest) = placeholder_re.as_ref().and_then(|re| re.find(&v).map(|m| m.as_str().to_string())) {
            return Err(AppError::Config(format!("参数 {rest} 未提供，无法执行该项配置")));
        }
        Ok(v)
    };

    for action in &item.actions {
        match action {
            Action::EnvSet { scope, set, path_append } => {
                env::backup_env(&format!("{}（写入环境变量前）", item.name))?;
                for (k, val) in set {
                    let key = fill(k)?;
                    // %VAR% 在写入前展开为绝对路径：普通 REG_SZ 不会像 PATH 一样被系统展开，
                    // 字面量 %LOCALAPPDATA%\pnpm 存进去后依赖它的程序拿不到真实路径
                    let value = env_norm::expand_env(&fill(val)?);
                    env::set_var(*scope, &key, &value)?;
                    logs.push(StepLog { ok: true, message: format!("设置环境变量 {key}={value}") });
                }
                for dir in path_append {
                    let dir = fill(dir)?;
                    let r = env::ensure_path_entry(*scope, &dir)?;
                    let msg = match r {
                        env::PathWrite::Skipped => {
                            format!("PATH 已由安装器配置，跳过：{dir}")
                        }
                        env::PathWrite::Appended => format!("PATH 追加：{dir}"),
                    };
                    logs.push(StepLog { ok: true, message: msg });
                }
                env::broadcast_change();
            }
            Action::Command { program, args, optional } => {
                let prog = fill(program)?;
                let argv: Vec<String> = args.iter().map(|a| fill(a)).collect::<AppResult<Vec<_>>>()?;
                match proc::run_program(&prog, &argv, &extra_dirs(item), Duration::from_secs(120)) {
                    Ok(out) if out.code == 0 => logs.push(StepLog {
                        ok: true,
                        message: format!("执行 {} 成功：{}", prog, out.combined()),
                    }),
                    Ok(out) => {
                        let msg = format!("执行 {} 失败（退出码 {}）：{}", prog, out.code, out.combined());
                        if *optional {
                            logs.push(StepLog { ok: false, message: format!("（可忽略）{msg}") });
                        } else {
                            return Err(AppError::Command(msg));
                        }
                    }
                    Err(e) => {
                        if *optional {
                            logs.push(StepLog { ok: false, message: format!("（可忽略）{e}") });
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            Action::TomlMerge { file, set, set_table } => {
                let path = PathBuf::from(env_norm::expand_env(&fill(file)?));
                // 预检待写入值：占位符缺失在改文件前报错
                for e in set {
                    fill(&e.value)?;
                }
                for e in set_table {
                    fill(&e.value)?;
                }
                let log = merge_toml(&path, set, set_table, &fill_raw)?;
                logs.push(log);
            }
            Action::SshKeygen { key_type, file, comment } => {
                let path = PathBuf::from(env_norm::expand_env(&fill(file)?));
                let log = ssh_keygen(&path, key_type, &fill(comment)?)?;
                logs.push(log);
            }
            Action::XmlMerge { file, root, container, child, id_tag, id_value, fields } => {
                let path = PathBuf::from(env_norm::expand_env(&fill(file)?));
                // 预检占位符，避免写坏文件
                fill(id_value)?;
                for v in fields.values() {
                    fill(v)?;
                }
                let pairs: Vec<(String, String)> = fields
                    .iter()
                    .map(|(k, v)| (fill(k).unwrap_or_default(), fill(v).unwrap_or_default()))
                    .collect();
                let changed = crate::xml::merge_into_file(
                    &path,
                    &fill(root)?,
                    container,
                    child,
                    id_tag,
                    &fill(id_value)?,
                    &pairs,
                )?;
                logs.push(StepLog {
                    ok: true,
                    message: if changed {
                        format!("已合并 XML 配置：{}", path.display())
                    } else {
                        format!("XML 配置已是期望内容，跳过：{}", path.display())
                    },
                });
            }
            Action::FileWrite { file, lines } => {
                let path = PathBuf::from(env_norm::expand_env(&fill(file)?));
                for l in lines {
                    fill(l)?;
                }
                write_whole_file(&path, lines, &fill_raw)?;
                logs.push(StepLog { ok: true, message: format!("已写入：{}", path.display()) });
            }
            Action::FileEnsureLines { file, lines } => {
                let path = PathBuf::from(env_norm::expand_env(&fill(file)?));
                for l in lines {
                    fill(l)?;
                }
                ensure_lines(&path, lines, &fill_raw)?;
                logs.push(StepLog { ok: true, message: format!("已确认配置行：{}", path.display()) });
            }
        }
    }
    Ok(logs)
}

/// 用 toml_edit 合并 TOML，保留原注释与既有内容。
pub fn merge_toml(
    path: &Path,
    set: &[crate::manifest::TomlSetEntry],
    set_table: &[crate::manifest::TomlSetTableEntry],
    fill: &dyn Fn(&str) -> String,
) -> AppResult<StepLog> {
    use toml_edit::{DocumentMut, Value};

    if path.exists() {
        let backup = path.with_extension("toml.bak");
        std::fs::copy(path, &backup)?;
    } else if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut doc: DocumentMut = if path.exists() {
        std::fs::read_to_string(path)?
            .parse::<DocumentMut>()
            .map_err(|e| AppError::Config(format!("{} 解析失败: {e}", path.display())))?
    } else {
        DocumentMut::new()
    };

    for entry in set {
        let value = Value::from(fill(&entry.value));
        set_dotted(&mut doc, path, &entry.path, value)?;
    }
    for entry in set_table {
        let value = Value::from(fill(&entry.value));
        set_dotted_table(&mut doc, path, &entry.path, &entry.key, value)?;
    }

    std::fs::write(path, doc.to_string())?;
    Ok(StepLog { ok: true, message: format!("已合并配置：{}", path.display()) })
}

/// 整文件写入（FileWrite 动作）。用于本工具自托管的文件（如 Gradle init.d 固定命名脚本），
/// 已存在时备份为 `<文件名>.bak`，缺失时自动创建父目录。
pub fn write_whole_file(path: &Path, lines: &[String], fill: &dyn Fn(&str) -> String) -> AppResult<()> {
    if path.exists() {
        let mut backup_name = path.as_os_str().to_owned();
        backup_name.push(".bak");
        std::fs::copy(path, Path::new(&backup_name))?;
    } else if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content = lines.iter().map(|l| fill(l)).collect::<Vec<_>>().join("\r\n");
    content.push_str("\r\n");
    std::fs::write(path, content)?;
    Ok(())
}

/// 行追加（FileEnsureLines 动作）。文件中（trim 后精确比较）不存在的行才追加，写前备份。
pub fn ensure_lines(path: &Path, lines: &[String], fill: &dyn Fn(&str) -> String) -> AppResult<()> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        String::new()
    };
    let have: Vec<String> = existing.lines().map(|l| l.trim().to_string()).collect();
    let missing: Vec<String> = lines
        .iter()
        .map(|l| fill(l))
        .filter(|l| !have.contains(&l.trim().to_string()))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    if path.exists() {
        let mut backup_name = path.as_os_str().to_owned();
        backup_name.push(".bak");
        std::fs::copy(path, Path::new(&backup_name))?;
    }
    let mut next = existing.clone();
    if !next.is_empty() && !next.ends_with('\n') {
        next.push_str("\r\n");
    }
    for l in &missing {
        next.push_str(l);
        next.push_str("\r\n");
    }
    std::fs::write(path, next)?;
    Ok(())
}

fn set_dotted(doc: &mut toml_edit::DocumentMut, path: &Path, dotted: &str, value: toml_edit::Value) -> AppResult<()> {
    let mut parts: Vec<&str> = dotted.split('.').collect();
    let key = parts.pop().ok_or_else(|| AppError::Config("空的路径".into()))?;
    let mut cur: &mut toml_edit::Item = doc.as_item_mut();
    for p in parts {
        // 中间段已被非表值占用时，toml_edit 的 IndexMut 会 panic（expect），这里显式报错
        if cur.get(p).map(|it| it.as_table_like().is_none()).unwrap_or(false) {
            return Err(AppError::Config(format!(
                "{dotted} 的中间段 \"{p}\" 已被非表值占用，无法写入；请手工修正 {}", path.display())));
        }
        if cur.get(p).is_none() {
            cur[p] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        cur = cur.get_mut(p).ok_or_else(|| AppError::Config(format!("无法创建表 {p}")))?;
    }
    // 已是表不能被单值整体覆盖
    if cur.get(key).map(|it| it.as_table_like().is_some()).unwrap_or(false) {
        return Err(AppError::Config(format!(
            "{dotted} 已是表，不能被单值覆盖；请手工修正 {}", path.display())));
    }
    cur[key] = toml_edit::Item::Value(value);
    Ok(())
}

fn set_dotted_table(
    doc: &mut toml_edit::DocumentMut,
    path: &Path,
    dotted: &str,
    key: &str,
    value: toml_edit::Value,
) -> AppResult<()> {
    let mut cur: &mut toml_edit::Item = doc.as_item_mut();
    for p in dotted.split('.') {
        if cur.get(p).map(|it| it.as_table_like().is_none()).unwrap_or(false) {
            return Err(AppError::Config(format!(
                "{dotted} 的中间段 \"{p}\" 已被非表值占用，无法写入；请手工修正 {}", path.display())));
        }
        if cur.get(p).is_none() {
            cur[p] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        cur = cur.get_mut(p).ok_or_else(|| AppError::Config(format!("无法创建表 {p}")))?;
    }
    if let Some(t) = cur.as_table_mut() {
        t[key] = toml_edit::Item::Value(value);
        Ok(())
    } else {
        Err(AppError::Config(format!("{dotted} 不是表；请手工修正 {}", path.display())))
    }
}

/// 生成 SSH 密钥。已有公钥时**不覆盖**，直接返回「复用」；
/// 只有私钥而缺公钥时，尝试用 `ssh-keygen -y` 从私钥重建公钥。
pub fn ssh_keygen(key_path: &Path, key_type: &str, comment: &str) -> AppResult<StepLog> {
    let pub_path = PathBuf::from(format!("{}.pub", key_path.display()));
    if pub_path.exists() {
        return Ok(StepLog {
            ok: true,
            message: format!("已存在密钥，复用不覆盖：{}", key_path.display()),
        });
    }
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ssh_keygen = proc::which("ssh-keygen", &[PathBuf::from("C:\\Windows\\System32\\OpenSSH")])
        .ok_or_else(|| AppError::Config("找不到 ssh-keygen，请先安装 OpenSSH 客户端".into()))?;

    if key_path.exists() {
        // 私钥在而 .pub 缺：用 ssh-keygen -y 从私钥重建公钥（私钥有口令时此路不通）
        let out = proc::run_capture(
            &ssh_keygen,
            &["-y".into(), "-f".into(), key_path.to_string_lossy().to_string()],
            Duration::from_secs(30),
        )?;
        if out.code != 0 || out.stdout.trim().is_empty() {
            return Err(AppError::Config(format!(
                "已存在私钥但缺少公钥，且无法自动重建（若私钥设有口令请手工处理）：{}",
                out.combined()
            )));
        }
        std::fs::write(&pub_path, format!("{}\n", out.stdout.trim()))?;
        return Ok(StepLog { ok: true, message: format!("已从现有私钥重建公钥：{}", pub_path.display()) });
    }

    let out = proc::run_capture(
        &ssh_keygen,
        &[
            "-t".into(),
            key_type.to_string(),
            "-C".into(),
            comment.to_string(),
            "-f".into(),
            key_path.to_string_lossy().to_string(),
            "-N".into(),
            String::new(),
        ],
        Duration::from_secs(60),
    )?;
    if out.code != 0 || !key_path.exists() {
        return Err(AppError::Config(format!("ssh-keygen 失败（退出码 {}）：{}", out.code, out.combined())));
    }
    Ok(StepLog { ok: true, message: format!("已生成新密钥：{}", key_path.display()) })
}

/// 读取公钥文本（供界面展示与复制）。
pub fn read_public_key(key_path: &Path) -> AppResult<String> {
    let pub_path = PathBuf::from(format!("{}.pub", key_path.display()));
    std::fs::read_to_string(&pub_path)
        .map(|s| s.trim().to_string())
        .map_err(|_| AppError::Config(format!("公钥不存在：{}", pub_path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{TomlSetEntry, TomlSetTableEntry};

    #[test]
    fn merge_preserves_comments_and_existing_keys() {
        let dir = std::env::temp_dir().join("devenv_cfg_toml");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("config.toml");
        std::fs::write(&f, "# 我的注释\n[net]\ngit-fetch-with-cli = true\n").unwrap();

        let set = vec![TomlSetEntry { path: "source.crates-io.replace-with".into(), value: "mirror".into() }];
        let set_table = vec![TomlSetTableEntry {
            path: "source.mirror".into(),
            key: "registry".into(),
            value: "sparse+{mirror_url}".into(),
        }];
        merge_toml(&f, &set, &set_table, &|s| s.replace("{mirror_url}", "https://rsproxy.cn/index/")).unwrap();

        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.contains("# 我的注释"), "注释应保留");
        assert!(text.contains("git-fetch-with-cli = true"), "既有键应保留");
        assert!(text.contains("replace-with = \"mirror\""));
        assert!(text.contains("sparse+https://rsproxy.cn/index/"));

        // 二次合并不重复插入
        merge_toml(&f, &set, &set_table, &|s| s.replace("{mirror_url}", "https://rsproxy.cn/index/")).unwrap();
        let text2 = std::fs::read_to_string(&f).unwrap();
        assert_eq!(text2.matches("replace-with").count(), 1, "二次合并不应重复");
    }

    #[test]
    fn merge_creates_file_when_missing() {
        let dir = std::env::temp_dir().join("devenv_cfg_toml_new");
        let _ = std::fs::remove_dir_all(&dir);
        let f = dir.join(".cargo").join("config.toml");
        let set = vec![TomlSetEntry { path: "source.crates-io.replace-with".into(), value: "mirror".into() }];
        merge_toml(&f, &set, &[], &|s| s.to_string()).unwrap();
        assert!(f.exists());
    }

    #[test]
    fn broken_toml_is_rejected_without_touching_original() {
        let dir = std::env::temp_dir().join("devenv_cfg_toml_bad");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("bad.toml");
        std::fs::write(&f, "this is = = broken").unwrap();
        let r = merge_toml(&f, &[], &[], &|s| s.to_string());
        assert!(r.is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "this is = = broken");
    }

    #[test]
    fn merge_rejects_non_table_intermediate() {
        let dir = std::env::temp_dir().join("devenv_cfg_toml_nti");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("config.toml");
        std::fs::write(&f, "source = \"x\"\n").unwrap();

        let set = vec![TomlSetEntry { path: "source.crates-io.replace-with".into(), value: "mirror".into() }];
        let r = merge_toml(&f, &set, &[], &|s: &str| s.to_string());
        assert!(r.is_err(), "中间段被非表值占用应报错而非 panic");
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("非表值"), "错误信息应包含 非表值：{msg}");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "source = \"x\"\n", "原文件不应被改动");
    }

    #[test]
    fn merge_rejects_value_overwriting_table() {
        let dir = std::env::temp_dir().join("devenv_cfg_toml_vot");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("config.toml");
        std::fs::write(&f, "[net]\ngit-fetch-with-cli = true\n").unwrap();

        let set = vec![TomlSetEntry { path: "net".into(), value: "x".into() }];
        let r = merge_toml(&f, &set, &[], &|s: &str| s.to_string());
        assert!(r.is_err(), "单值覆盖既有表应报错");
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("已是表"), "错误信息应包含 已是表：{msg}");
        assert!(
            std::fs::read_to_string(&f).unwrap().contains("git-fetch-with-cli = true"),
            "原表内容不应被改动"
        );
    }

    #[test]
    fn file_write_overwrites_with_backup_and_ensure_lines_appends_once() {
        let dir = std::env::temp_dir().join("devenv_cfg_file_actions");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        // FileWrite：无父目录自动创建、内容带 CRLF、已有文件留 .bak
        let f = dir.join("init.d").join("devenv-mirror.gradle");
        write_whole_file(&f, &["// 注释".into(), "allprojects {}".into()], &|s| s.to_string()).unwrap();
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.contains("allprojects {}"));
        write_whole_file(&f, &["// v2".into()], &|s| s.to_string()).unwrap();
        assert!(f.as_os_str().to_string_lossy().ends_with(".gradle"));
        assert!(
            std::path::Path::new(&format!("{}.bak", f.display())).exists(),
            "二次写入应留备份"
        );

        // FileEnsureLines：追加缺失行；重复执行不再追加
        let r = dir.join(".Rprofile");
        std::fs::write(&r, "options(repos = c(CRAN = \"https://old.example.com\"))\n").unwrap();
        ensure_lines(&r, &["options(repos = c(CRAN = \"https://new.example.com\"))".into()], &|s| s.to_string())
            .unwrap();
        let text = std::fs::read_to_string(&r).unwrap();
        assert_eq!(text.matches("options(repos").count(), 2, "新行应追加在旧行之后");
        ensure_lines(&r, &["options(repos = c(CRAN = \"https://new.example.com\"))".into()], &|s| s.to_string())
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&r).unwrap().matches("options(repos").count(),
            2,
            "重复执行不应重复追加"
        );

        // FileEnsureLines：trim 后比较，空白差异不算缺失
        let r2 = dir.join(".Rprofile2");
        std::fs::write(&r2, "  options(prompt = \">\")  \n").unwrap();
        ensure_lines(&r2, &["options(prompt = \">\")".into()], &|s| s.to_string()).unwrap();
        assert_eq!(std::fs::read_to_string(&r2).unwrap().matches("options(prompt").count(), 1);
    }

    #[test]
    fn missing_param_is_reported_before_execution() {
        use crate::manifest::{Kind, Source};
        let item = Item {
            id: "t".into(),
            name: "T".into(),
            category: "c".into(),
            source: Source::Config,
            kind: Kind::Config,
            provider: None,
            path: None,
            archive: None,
            requires: vec![],
            actions: vec![Action::Command {
                program: "cmd".into(),
                args: vec!["/C".into(), "echo".into(), "{prefix_dir}".into()],
                optional: false,
            }],
            mirrors: vec![],
            detect: None,
            env: None,
            verify: vec![],
            note: None,
            expect_version: None,
        };
        let r = run_actions(&item, None, &Params::default());
        assert!(r.is_err(), "缺参数应报错而非静默执行");
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("未提供"), "错误信息应包含 未提供：{msg}");
        assert!(msg.contains("{prefix_dir}"), "错误信息应指明缺失的参数：{msg}");
    }
}

#[cfg(test)]
mod ssh_tests {
    use super::*;

    #[test]
    fn existing_key_is_not_overwritten() {
        let dir = std::env::temp_dir().join("devenv_ssh_test");
        let _ = std::fs::create_dir_all(&dir);
        let key = dir.join("id_ed25519");
        std::fs::write(&key, "PRIVATE").unwrap();
        // 私钥与公钥都在：走「复用不覆盖」，不调 ssh-keygen
        std::fs::write(dir.join("id_ed25519.pub"), "ssh-ed25519 AAA fake").unwrap();
        let log = ssh_keygen(&key, "ed25519", "me@example.com").unwrap();
        assert!(log.message.contains("复用"));
        assert_eq!(std::fs::read_to_string(&key).unwrap(), "PRIVATE");
    }

    #[test]
    fn private_key_without_pub_reports_rebuild_failure() {
        let dir = std::env::temp_dir().join("devenv_ssh_test3");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let key = dir.join("id_ed25519_broken");
        // 假私钥：ssh-keygen -y 解析失败，走「无法重建」错误分支
        std::fs::write(&key, "not a real key").unwrap();
        let r = ssh_keygen(&key, "ed25519", "me@example.com");
        assert!(r.is_err(), "假私钥无法重建公钥，应报错");
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("重建"), "错误信息应包含 重建：{msg}");
        assert!(!dir.join("id_ed25519_broken.pub").exists(), "不应留下重建出的 .pub");
    }

    #[test]
    fn missing_public_key_reports_error() {
        let dir = std::env::temp_dir().join("devenv_ssh_test2");
        let _ = std::fs::create_dir_all(&dir);
        let key = dir.join("id_ed25519_none");
        assert!(read_public_key(&key).is_err());
    }
}
