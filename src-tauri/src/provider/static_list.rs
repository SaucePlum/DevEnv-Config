//! 手工维护的版本列表（MySQL）。官网下载页对程序返回 403，无法自动获取版本。

use super::{DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

pub struct StaticListProvider;

fn versions_of(item: &Item) -> &[crate::manifest::StaticVersion] {
    match item.provider.as_ref() {
        Some(ProviderDef::StaticList { versions }) => versions,
        _ => &[],
    }
}

pub fn to_version_infos(vs: &[crate::manifest::StaticVersion], prefer: Prefer) -> Vec<VersionInfo> {
    let mut out: Vec<VersionInfo> = vs
        .iter()
        .map(|v| VersionInfo {
            version: v.version.clone(),
            lts: false,
            lts_name: None,
            date: None,
            recommended: false,
        })
        .collect();
    super::sort_and_mark(&mut out, prefer);
    out
}

#[async_trait::async_trait]
impl VersionProvider for StaticListProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = Prefer::Latest;
        Ok(to_version_infos(versions_of(item), prefer))
    }

    async fn resolve(&self, item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let v = versions_of(item)
            .iter()
            .find(|v| v.version == version)
            .ok_or_else(|| ProviderError::NoAsset(version.to_string()))?;
        let file_name = v.url.rsplit('/').next().unwrap_or("download.msi").to_string();
        Ok(vec![DownloadTarget {
            file_name,
            urls: vec![v.url.clone()],
            sha256: v.sha256.clone(),
            md5: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::StaticVersion;

    #[test]
    fn versions_listed_with_latest_recommended() {
        let vs = vec![
            StaticVersion { version: "8.4.6".into(), url: "u1".into(), sha256: None },
            StaticVersion { version: "8.0.40".into(), url: "u2".into(), sha256: None },
        ];
        let list = to_version_infos(&vs, Prefer::Latest);
        assert_eq!(list[0].version, "8.4.6");
        assert!(list[0].recommended);
    }

    #[test]
    fn file_name_comes_from_url() {
        let v = StaticVersion { version: "8.4.6".into(), url: "https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.6-winx64.msi".into(), sha256: None };
        assert_eq!(v.url.rsplit('/').next().unwrap(), "mysql-8.4.6-winx64.msi");
    }
}
