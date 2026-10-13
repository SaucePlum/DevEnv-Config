//! 内置资源（payload）定位。开发期直接读 `src-tauri/resources/payload`，
//! 避免 Tauri 在 dev 模式复制 195 MB；发布期走资源目录。

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[cfg(not(debug_assertions))]
use tauri::Manager;

pub fn payload_root(_app: &AppHandle) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("payload")
    }
    #[cfg(not(debug_assertions))]
    {
        _app.path()
            .resolve("resources/payload", tauri::path::BaseDirectory::Resource)
            .expect("解析内置资源目录失败")
    }
}

/// 解析清单里的相对路径（形如 `build/cmake/xxx.msi`），并校验存在且非空。
pub fn resolve_local(app: &AppHandle, rel: &str) -> AppResult<PathBuf> {
    let p: PathBuf = payload_root(app)
        .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    ensure_non_empty(&p).map_err(|_| AppError::MissingResource(rel.to_string()))?;
    Ok(p)
}

fn ensure_non_empty(p: &Path) -> Result<(), ()> {
    let meta = std::fs::metadata(p).map_err(|_| ())?;
    if meta.is_file() && meta.len() > 0 {
        Ok(())
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_non_empty_detects_missing_and_empty() {
        let dir = std::env::temp_dir().join("devenv_payload_test");
        let _ = std::fs::create_dir_all(&dir);
        assert!(ensure_non_empty(&dir.join("nope.bin")).is_err());
        let empty = dir.join("empty.bin");
        std::fs::write(&empty, b"").unwrap();
        assert!(ensure_non_empty(&empty).is_err());
        let ok = dir.join("ok.bin");
        std::fs::write(&ok, b"x").unwrap();
        assert!(ensure_non_empty(&ok).is_ok());
    }
}
