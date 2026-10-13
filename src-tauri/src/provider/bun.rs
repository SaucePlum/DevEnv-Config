//! Bun 版本源：GitHub Releases。资产名在所有 release 中固定为 `bun-windows-x64.zip`，
//! 版本号只能取自 tag（`bun-v1.4.3` → `1.4.3`），与 github_release 适配器的
//! 「资产名反解版本」模式不同，故单独实现。
//! 回落镜像：npmmirror `-binary/bun/bun-v{version}/bun-windows-x64.zip`（实测可用）。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const RELEASES: &str = "https://api.github.com/repos/oven-sh/bun/releases?per_page=40";
const ASSET_NAME: &str = "bun-windows-x64.zip";

pub struct BunProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::Bun { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// `bun-v1.4.3` → `1.4.3`。
pub fn version_from_tag(tag: &str) -> String {
    tag.trim().trim_start_matches("bun-").trim_start_matches('v').to_string()
}

/// 镜像直链：npmmirror 以 `bun-v{版本}` 为目录名。
pub fn mirror_url(version: &str) -> String {
    format!("https://registry.npmmirror.com/-/binary/bun/bun-v{version}/{ASSET_NAME}")
}

/// 纯解析：取资产里含 `bun-windows-x64.zip` 的正式 release，按版本降序。
pub fn parse_releases(json: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let rels: Vec<GhRelease> =
        serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let mut out: Vec<VersionInfo> = rels
        .into_iter()
        .filter(|r| !r.prerelease && !r.draft)
        .filter(|r| r.assets.iter().any(|a| a.name == ASSET_NAME))
        .map(|r| VersionInfo {
            version: version_from_tag(&r.tag_name),
            lts: false,
            lts_name: None,
            date: if r.published_at.is_empty() { None } else { Some(r.published_at) },
            recommended: false,
        })
        .collect();
    out.sort_by(|a, b| super::cmp_version(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

#[async_trait::async_trait]
impl VersionProvider for BunProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text(RELEASES).await?;
        let list = parse_releases(&body, prefer)?;
        // 列表结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index("没有解析到可用的 Bun 版本".into()));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let body = crate::http::get_text(RELEASES).await?;
        let rels: Vec<GhRelease> =
            serde_json::from_str(&body).map_err(|e| ProviderError::Parse(e.to_string()))?;
        for r in rels {
            if r.prerelease || r.draft || version_from_tag(&r.tag_name) != version {
                continue;
            }
            let asset = r
                .assets
                .iter()
                .find(|a| a.name == ASSET_NAME)
                .ok_or_else(|| ProviderError::NoAsset(version.to_string()))?;
            return Ok(vec![DownloadTarget {
                file_name: ASSET_NAME.to_string(),
                urls: vec![asset.browser_download_url.clone(), mirror_url(version)],
                sha256: None,
                md5: None,
            }]);
        }
        Err(ProviderError::NoAsset(version.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[
      {"tag_name":"bun-v1.4.3","prerelease":false,"draft":false,"published_at":"2026-09-01T00:00:00Z",
       "assets":[
         {"name":"bun-windows-x64-baseline.zip","browser_download_url":"https://github.com/x/baseline.zip"},
         {"name":"bun-windows-x64.zip","browser_download_url":"https://github.com/x/bun-windows-x64.zip"},
         {"name":"bun-v1.4.3.zip","browser_download_url":"https://github.com/x/src.zip"}
       ]},
      {"tag_name":"bun-v1.5.0","prerelease":true,"draft":false,"published_at":"","assets":[{"name":"bun-windows-x64.zip","browser_download_url":"u"}]},
      {"tag_name":"bun-v1.3.9","prerelease":false,"draft":false,"published_at":"","assets":[{"name":"bun-windows-x64.zip","browser_download_url":"u2"}]}
    ]"#;

    #[test]
    fn strips_bun_v_prefix_and_skips_prerelease() {
        let list = parse_releases(JSON, Prefer::Latest).unwrap();
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["1.4.3", "1.3.9"],
            "预发布版不应出现"
        );
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "1.4.3");
    }

    #[test]
    fn empty_and_malformed_inputs() {
        assert!(parse_releases("[]", Prefer::Latest).unwrap().is_empty());
        assert!(parse_releases("not json", Prefer::Latest).is_err());
    }

    #[test]
    fn mirror_url_uses_bun_v_dir() {
        assert_eq!(
            mirror_url("1.4.3"),
            "https://registry.npmmirror.com/-/binary/bun/bun-v1.4.3/bun-windows-x64.zip"
        );
    }

    #[tokio::test]
    async fn resolve_uses_manifest_item() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("bun").unwrap().clone();
        // 走真实 GitHub API，取列表里能稳定存在的旧版本；
        // 未认证配额（60 次/小时/IP）耗尽时返回 403，跳过而非误报失败
        let t = match BunProvider.resolve(&item, "1.4.3").await {
            Ok(t) => t,
            Err(e) if e.to_string().contains("403") => {
                eprintln!("GitHub API 限流（403），跳过该用例");
                return;
            }
            Err(e) => panic!("解析失败: {e}"),
        };
        assert_eq!(t[0].file_name, "bun-windows-x64.zip");
        assert!(t[0].urls[0].contains("bun-windows-x64.zip"), "官方直链应指向目标资产");
        assert!(t[0].urls[1].contains("registry.npmmirror.com"));
    }
}
