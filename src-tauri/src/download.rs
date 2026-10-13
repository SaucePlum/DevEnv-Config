//! 下载安装包：官网优先 → 镜像回退；单地址重试 1 次；命中缓存即复用；有官方校验值则校验。
//! 设计依据：设计文档第 5 节回退规则、第 18 节异常处理。

use crate::error::{AppError, AppResult};
use crate::provider::DownloadTarget;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

/// 工具数据目录：`%LOCALAPPDATA%\DevEnv`
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DevEnv")
}

/// 下载缓存目录：`%LOCALAPPDATA%\DevEnv\cache`
pub fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}

pub fn sha256_file(path: &Path) -> AppResult<String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(hex::encode(h.finalize()))
}

pub fn md5_file(path: &Path) -> AppResult<String> {
    use md5::{Digest, Md5};
    let mut f = std::fs::File::open(path)?;
    let mut h = Md5::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(hex::encode(h.finalize()))
}

pub fn verify_checksums(path: &Path, sha256: Option<&str>, md5: Option<&str>) -> AppResult<()> {
    if let Some(expect) = sha256 {
        let got = sha256_file(path)?;
        if !got.eq_ignore_ascii_case(expect.trim()) {
            return Err(AppError::Checksum(crate::tr!("SHA256 不匹配：期望 {0}，实际 {1}", "SHA256 mismatch: expected {0}, got {1}", expect, got)));
        }
    }
    if let Some(expect) = md5 {
        let got = md5_file(path)?;
        if !got.eq_ignore_ascii_case(expect.trim()) {
            return Err(AppError::Checksum(crate::tr!("MD5 不匹配：期望 {0}，实际 {1}", "MD5 mismatch: expected {0}, got {1}", expect, got)));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct DownloadReport {
    pub path: PathBuf,
    pub attempted: Vec<String>,
    pub reused_cache: bool,
}

/// 下载单个目标。`progress(已下载字节, 总字节)` 由调用方转成界面进度。
pub async fn fetch(
    target: &DownloadTarget,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> AppResult<DownloadReport> {
    std::fs::create_dir_all(cache_dir())?;
    let dest = cache_dir().join(&target.file_name);

    if dest.is_file() {
        if verify_checksums(&dest, target.sha256.as_deref(), target.md5.as_deref()).is_ok() {
            return Ok(DownloadReport { path: dest, attempted: vec![], reused_cache: true });
        }
        let _ = std::fs::remove_file(&dest);
    }

    let mut attempted = Vec::new();
    let mut last_err = String::new();

    for url in &target.urls {
        for _ in 0..2 {
            attempted.push(url.clone());
            match download_one(url, &dest, progress).await {
                Ok(()) => match verify_checksums(&dest, target.sha256.as_deref(), target.md5.as_deref()) {
                    Ok(()) => return Ok(DownloadReport { path: dest, attempted, reused_cache: false }),
                    Err(e) => {
                        last_err = e.to_string();
                        let _ = std::fs::remove_file(&dest);
                        // 校验失败：换下一个地址/镜像重试
                    }
                },
                Err(e) => {
                    last_err = e.to_string();
                    let _ = std::fs::remove_file(part_path(&dest));
                }
            }
        }
    }

    let mut seen: Vec<&String> = Vec::new();
    for u in &attempted { if !seen.contains(&u) { seen.push(u); } }
    let list = seen.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("; ");
    Err(AppError::Download(crate::tr!(
        "{0} 下载失败：共尝试 {1} 次（地址：{2}）。最后错误：{3}",
        "{0} download failed: tried {1} time(s) (URLs: {2}). Last error: {3}",
        target.file_name,
        attempted.len(),
        list,
        last_err
    )))
}

/// 临时文件路径：完整文件名 + ".part" 后缀，避免 with_extension 截断含点文件名
fn part_path(dest: &Path) -> PathBuf {
    dest.with_file_name(format!(
        "{}.part",
        dest.file_name().and_then(|s| s.to_str()).unwrap_or("download")
    ))
}

async fn download_one(
    url: &str,
    dest: &Path,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> AppResult<()> {
    let resp = crate::http::client()
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Download(crate::tr!("{0} 连接失败: {1}", "Failed to connect to {0}: {1}", url, e)))?;
    if !resp.status().is_success() {
        return Err(AppError::Download(crate::tr!("{0} 返回 HTTP {1}", "{0} returned HTTP {1}", url, resp.status().as_u16())));
    }
    let total = resp.content_length();
    let part = part_path(dest);
    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = resp.bytes_stream();
    let mut done: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Download(crate::tr!("{0} 传输中断: {1}", "Transfer interrupted for {0}: {1}", url, e)))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    drop(file);
    std::fs::rename(&part, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn md5_matches_known_value() {
        let p = tmp("devenv_md5_abc.bin", b"abc");
        assert_eq!(md5_file(&p).unwrap(), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn sha256_matches_known_value() {
        let p = tmp("devenv_sha_empty.bin", b"");
        assert_eq!(
            sha256_file(&p).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn verify_checksums_accepts_matching_and_rejects_mismatch() {
        let p = tmp("devenv_verify.bin", b"abc");
        assert!(verify_checksums(&p, None, Some("900150983CD24FB0D6963F7D28E17F72")).is_ok());
        assert!(verify_checksums(&p, None, Some("deadbeef")).is_err());
    }

    #[test]
    fn part_path_keeps_full_file_name() {
        // with_extension("part") 会把 v1.2.3 截断成 v1.2.part；part_path 必须保留完整名
        assert_eq!(
            part_path(&Path::new("C:\\cache").join("node-v1.2.3.zip")),
            Path::new("C:\\cache").join("node-v1.2.3.zip.part")
        );
        assert_eq!(
            part_path(&Path::new("C:\\cache").join("v1.2.3")),
            Path::new("C:\\cache").join("v1.2.3.part")
        );
    }

    #[test]
    fn cache_and_data_dirs_are_under_localappdata() {
        assert!(cache_dir().ends_with("DevEnv\\cache") || cache_dir().ends_with("DevEnv/cache"));
    }
}
