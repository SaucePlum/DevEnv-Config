//! Rust 版本源：只负责提供 `rustup-init.exe`；工具链版本交给 rustup 自身（stable 通道）。

use super::{DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, ProviderDef};

pub struct RustupProvider;

fn cfg(item: &Item) -> Result<(String, Option<String>), ProviderError> {
    match item.provider.as_ref() {
        Some(ProviderDef::Rustup { url, mirror }) => Ok((url.clone(), mirror.clone())),
        _ => Err(ProviderError::NoProvider),
    }
}

#[async_trait::async_trait]
impl VersionProvider for RustupProvider {
    async fn list_versions(&self, _item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        Ok(vec![VersionInfo {
            version: "stable".to_string(),
            lts: false,
            lts_name: None,
            date: None,
            recommended: true,
        }])
    }

    async fn resolve(&self, item: &Item, _version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let (url, mirror) = cfg(item)?;
        let mut urls = vec![url];
        if let Some(m) = mirror {
            urls.push(m);
        }
        Ok(vec![DownloadTarget {
            file_name: "rustup-init.exe".to_string(),
            urls,
            sha256: None,
            md5: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_puts_official_first_then_mirror() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("rust").unwrap().clone();
        let t = RustupProvider.resolve(&item, "stable").await.unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file_name, "rustup-init.exe");
        assert!(t[0].urls[0].contains("static.rust-lang.org"), "官方地址应在首位");
        assert!(t[0].urls[1].contains("rsproxy"), "镜像应在官方之后");
    }
}
