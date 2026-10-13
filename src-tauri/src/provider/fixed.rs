//! 固定地址项（VC 运行库、VS 2022）：无版本可选，始终取最新。

use super::{DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, ProviderDef};

pub struct FixedProvider;

fn urls_of(item: &Item) -> Vec<String> {
    match item.provider.as_ref() {
        Some(ProviderDef::Fixed { urls }) => urls.clone(),
        _ => vec![],
    }
}

/// 固定项只暴露一个占位版本，界面据此始终选中它。
pub fn single_version(item: &Item) -> VersionInfo {
    VersionInfo {
        version: item.expect_version.clone().unwrap_or_else(|| crate::tr!("最新", "latest")),
        lts: false,
        lts_name: None,
        date: None,
        recommended: true,
    }
}

#[async_trait::async_trait]
impl VersionProvider for FixedProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        Ok(vec![single_version(item)])
    }

    async fn resolve(&self, item: &Item, _version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let urls = urls_of(item);
        if urls.is_empty() {
            return Err(ProviderError::NoAsset(item.id.clone()));
        }
        // 每个 URL 一个待下载文件（VC 运行库有 x64 与 x86 两个）
        Ok(urls
            .into_iter()
            .map(|u| {
                let file_name = u.rsplit('/').next().unwrap_or("download.exe").to_string();
                DownloadTarget { file_name, urls: vec![u], sha256: None, md5: None }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vcredist_item() -> Item {
        crate::manifest::Manifest::load().unwrap().get("vcredist").unwrap().clone()
    }

    #[test]
    fn exposes_single_recommended_version() {
        let v = single_version(&vcredist_item());
        assert!(v.recommended);
    }

    #[tokio::test]
    async fn resolve_returns_one_target_per_url() {
        let item = vcredist_item();
        let t = FixedProvider.resolve(&item, "最新").await.unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].file_name, "vc_redist.x64.exe");
        assert_eq!(t[1].file_name, "vc_redist.x86.exe");
    }
}
