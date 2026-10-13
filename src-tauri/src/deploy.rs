//! 免安装类投放：解压到安装目录所在卷的临时目录 → 移动到安装目录 → 清理。
//! 设计依据：设计文档第 9 节。

use crate::error::{AppError, AppResult};
use crate::proc;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TAR_TIMEOUT: Duration = Duration::from_secs(60 * 30);

/// 安装目录所在卷的根，用于挑一个同卷的临时目录（同卷移动 = 重命名，瞬时完成）。
/// 入参应为绝对路径；盘符相对路径（如 `D:foo`）产生的临时目录依赖进程 CWD，不受支持，
/// 此时退回前缀本体（行为与原实现一致，调用方应始终传绝对路径）。
fn volume_root(dir: &Path) -> PathBuf {
    let mut comps = dir.components();
    match (comps.next(), comps.next()) {
        (Some(std::path::Component::Prefix(p)), Some(std::path::Component::RootDir)) => {
            // 带上根分隔符，得到绝对卷根（如 "C:\"），避免拼出 "C:DevEnv.tmp" 这类盘符相对路径
            let mut s = p.as_os_str().to_os_string();
            s.push(std::path::MAIN_SEPARATOR_STR);
            PathBuf::from(s)
        }
        (Some(std::path::Component::Prefix(p)), _) => PathBuf::from(p.as_os_str()),
        _ => PathBuf::from("C:\\"),
    }
}

pub fn temp_dir_for(install_dir: &Path, item_id: &str) -> PathBuf {
    let mut root = volume_root(install_dir);
    root.push("DevEnv.tmp");
    root.push(item_id);
    root
}

/// 用 `tar -tf` 列出条目，判断是否只有一个顶层目录（决定是否 `--strip-components=1`）。
pub fn has_single_top_level(archive: &Path) -> AppResult<bool> {
    let out = proc::run_capture(
        &proc::which("tar", &[]).ok_or_else(|| AppError::Extract("找不到系统 tar.exe".into()))?,
        &["-tf".to_string(), archive.to_string_lossy().to_string()],
        Duration::from_secs(120),
    )?;
    if out.code != 0 {
        return Err(AppError::Extract(format!("tar 列出失败: {}", out.combined())));
    }
    let mut tops: Vec<String> = Vec::new();
    for line in out.stdout.lines() {
        let line = line.trim().replace('\\', "/");
        if line.is_empty() {
            continue;
        }
        if let Some(first) = line.split('/').next() {
            if !first.is_empty() && !tops.iter().any(|t| t == first) {
                tops.push(first.to_string());
            }
        }
    }
    Ok(tops.len() == 1)
}

/// 安装目录是否已存在且非空（界面据此提示覆盖确认）。
pub fn dir_non_empty(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut it) => it.next().is_some(),
        Err(_) => false,
    }
}

/// 执行投放。`progress` 用于向界面报告阶段。应在阻塞线程调用。
pub fn deploy_archive(
    archive: &Path,
    install_dir: &Path,
    item_id: &str,
    progress: &dyn Fn(&str, Option<f64>),
) -> AppResult<()> {
    progress("准备解压", Some(0.05));

    let temp = temp_dir_for(install_dir, item_id);
    if temp.exists() {
        let _ = std::fs::remove_dir_all(&temp);
    }
    std::fs::create_dir_all(&temp)?;

    let result = (|| -> AppResult<()> {
        let strip = has_single_top_level(archive)?;
        let tar = proc::which("tar", &[]).ok_or_else(|| AppError::Extract("找不到系统 tar.exe".into()))?;

        let mut args = vec![
            "-xf".to_string(),
            archive.to_string_lossy().to_string(),
            "-C".to_string(),
            temp.to_string_lossy().to_string(),
        ];
        if strip {
            args.push("--strip-components=1".to_string());
        }

        progress("解压中", Some(0.35));
        let out = proc::run_capture(&tar, &args, TAR_TIMEOUT)?;
        if out.code != 0 {
            return Err(AppError::Extract(format!("tar 解压失败（退出码 {}）: {}", out.code, out.combined())));
        }

        progress("移动到安装目录", Some(0.8));
        std::fs::create_dir_all(install_dir)?;
        move_contents(&temp, install_dir)?;
        Ok(())
    })();

    // 无论成败都清理临时目录（失败时不留半成品）
    let _ = std::fs::remove_dir_all(&temp);
    if result.is_ok() {
        progress("完成", Some(1.0));
    } else {
        progress("已清理临时文件", None);
    }
    result
}

/// 把 src 下的直接子项移动到 dst；跨卷时退回 robocopy。
fn move_contents(src: &Path, dst: &Path) -> AppResult<()> {
    let mut fallback: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if to.exists() {
            // 目标被占用时立即失败，避免 robocopy 与残留内容混合出损坏的安装目录
            let removed = if to.is_dir() {
                std::fs::remove_dir_all(&to).is_ok()
            } else {
                std::fs::remove_file(&to).is_ok()
            };
            if !removed {
                return Err(AppError::Extract(format!(
                    "{} 被占用，无法覆盖，请关闭相关程序后重试",
                    to.display()
                )));
            }
        }
        if std::fs::rename(&from, &to).is_err() {
            fallback.push(from);
        }
    }
    for from in fallback {
        let robocopy = proc::which("robocopy", &[]).ok_or_else(|| AppError::Extract("跨卷移动失败且找不到 robocopy".into()))?;
        let out = proc::run_capture(
            &robocopy,
            &[
                from.to_string_lossy().to_string(),
                dst.to_string_lossy().to_string(),
                "/E".into(),
                "/MOVE".into(),
                "/MT:16".into(),
                "/NFL".into(),
                "/NDL".into(),
                "/NJH".into(),
                "/NJS".into(),
            ],
            TAR_TIMEOUT,
        )?;
        // robocopy 退出码 < 8 视为成功
        if out.code >= 8 {
            return Err(AppError::Extract(format!("robocopy 复制失败（退出码 {}）", out.code)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_root_of_c_path() {
        assert_eq!(volume_root(Path::new("C:\\Apps\\Tools")), PathBuf::from("C:\\"));
    }

    #[test]
    fn temp_dir_is_on_same_volume_as_install_dir() {
        let t = temp_dir_for(Path::new("D:\\Tools\\redis"), "redis");
        assert_eq!(volume_root(&t), volume_root(Path::new("D:\\Tools\\redis")));
        assert!(t.ends_with("redis"));
    }

    #[test]
    fn dir_non_empty_detects_content() {
        let d = std::env::temp_dir().join("devenv_deploy_nonempty");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert!(!dir_non_empty(&d));
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        assert!(dir_non_empty(&d));
        let _ = std::fs::remove_dir_all(&d);
    }
}
