//! GitHub Releases 版本源。用产物名模板反向匹配 release 资产，从而拿到真实版本号。
//! 例如 Git 的模板 `Git-{version}-64-bit.exe` 能从 `Git-2.47.1.2-64-bit.exe` 反解出 `2.47.1.2`。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

pub struct GithubReleaseProvider;

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

struct GhCfg {
    repo: String,
    asset: String,
    mirror: Option<String>,
    prefer: Prefer,
}

fn cfg(item: &Item) -> Result<GhCfg, ProviderError> {
    match item.provider.as_ref() {
        Some(ProviderDef::GithubRelease { repo, asset, mirror, prefer }) => Ok(GhCfg {
            repo: repo.clone(),
            asset: asset.clone(),
            mirror: mirror.clone(),
            prefer: *prefer,
        }),
        _ => Err(ProviderError::NoProvider),
    }
}

/// 由产物名模板得到匹配正则，`{version}` 变成命名捕获组。
pub fn asset_regex(tpl: &str) -> Result<regex::Regex, ProviderError> {
    let escaped = regex::escape(tpl)
        .replace(r"\{version\}", r"(?P<version>\d[\w\.\-]*)")
        .replace(r"\{tag\}", r"(?P<tag>[^/]+)");
    regex::Regex::new(&format!("^{escaped}$")).map_err(|e| ProviderError::Parse(e.to_string()))
}

/// 纯解析：从 GitHub releases JSON 中筛出资产名匹配模板的版本。
pub fn parse_releases(json: &str, asset_tpl: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let re = asset_regex(asset_tpl)?;
    let rels: Vec<GhRelease> =
        serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let mut out = Vec::new();
    for r in rels {
        if r.prerelease || r.draft {
            continue;
        }
        for a in &r.assets {
            if let Some(caps) = re.captures(&a.name) {
                let version = caps.name("version").map(|m| m.as_str().to_string()).unwrap_or_else(|| {
                    r.tag_name.trim_start_matches('v').to_string()
                });
                out.push(VersionInfo {
                    version,
                    lts: false,
                    lts_name: None,
                    date: if r.published_at.is_empty() { None } else { Some(r.published_at.clone()) },
                    recommended: false,
                });
                break;
            }
        }
    }
    out.sort_by(|a, b| super::cmp_version(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

#[async_trait::async_trait]
impl VersionProvider for GithubReleaseProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let c = cfg(item)?;
        let url = format!("https://api.github.com/repos/{}/releases?per_page=40", c.repo);
        let body = crate::http::get_text(&url).await?;
        parse_releases(&body, &c.asset, c.prefer)
    }

    async fn resolve(&self, item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let c = cfg(item)?;
        // 重新拉一次 releases，拿到该版本对应的真实 tag 与资产直链
        let url = format!("https://api.github.com/repos/{}/releases?per_page=40", c.repo);
        let body = crate::http::get_text(&url).await?;
        let re = asset_regex(&c.asset)?;
        let rels: Vec<GhRelease> =
            serde_json::from_str(&body).map_err(|e| ProviderError::Parse(e.to_string()))?;
        for r in rels {
            if r.prerelease || r.draft {
                continue;
            }
            for a in &r.assets {
                if let Some(caps) = re.captures(&a.name) {
                    let matched = caps.name("version").map(|m| m.as_str()).unwrap_or("");
                    if matched != version {
                        continue;
                    }
                    let tag = r.tag_name.clone();
                    let mut urls = vec![a.browser_download_url.clone()];
                    if let Some(m) = &c.mirror {
                        urls.push(super::fill_url_template(m, version, &tag));
                    }
                    return Ok(vec![DownloadTarget {
                        file_name: a.name.clone(),
                        urls,
                        sha256: None,
                        md5: None,
                    }]);
                }
            }
        }
        Err(ProviderError::NoAsset(version.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIT_JSON: &str = r#"[
      {"tag_name":"v2.55.0.windows.1","prerelease":false,"draft":false,"published_at":"2026-09-01T00:00:00Z",
       "assets":[{"name":"Git-2.55.0-64-bit.exe","browser_download_url":"https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.1/Git-2.55.0-64-bit.exe"}]},
      {"tag_name":"v2.54.0.windows.1","prerelease":false,"draft":false,"published_at":"2026-08-01T00:00:00Z",
       "assets":[{"name":"Git-2.54.0-64-bit.exe","browser_download_url":"https://x/Git-2.54.0-64-bit.exe"}]}
    ]"#;

    const REDIS_JSON: &str = r#"[
      {"tag_name":"8.10.2","prerelease":false,"draft":false,"published_at":"2026-09-10T00:00:00Z",
       "assets":[{"name":"Redis-8.10.2-Windows-x64-cygwin-with-Service.zip","browser_download_url":"https://github.com/redis-windows/redis-windows/releases/download/8.10.2/Redis-8.10.2-Windows-x64-cygwin-with-Service.zip"}]}
    ]"#;

    #[test]
    fn git_versions_extracted_from_asset_names() {
        let list = parse_releases(GIT_JSON, "Git-{version}-64-bit.exe", Prefer::Latest).unwrap();
        let names: Vec<&str> = list.iter().map(|v| v.version.as_str()).collect();
        assert_eq!(names, ["2.55.0", "2.54.0"]);
        assert!(list[0].recommended);
    }

    #[test]
    fn prerelease_is_skipped() {
        let json = r#"[{"tag_name":"v1","prerelease":true,"draft":false,"published_at":"","assets":[{"name":"Git-1.0.0-64-bit.exe","browser_download_url":"u"}]}]"#;
        let list = parse_releases(json, "Git-{version}-64-bit.exe", Prefer::Latest).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn redis_versions_use_plain_asset_template() {
        let list = parse_releases(REDIS_JSON, "Redis-{version}-Windows-x64-cygwin-with-Service.zip", Prefer::Latest).unwrap();
        assert_eq!(list[0].version, "8.10.2");
    }

    #[test]
    fn non_matching_assets_are_ignored() {
        let json = r#"[{"tag_name":"v1","prerelease":false,"draft":false,"published_at":"","assets":[{"name":"other.zip","browser_download_url":"u"}]}]"#;
        assert!(parse_releases(json, "Git-{version}-64-bit.exe", Prefer::Latest).unwrap().is_empty());
    }

    #[test]
    fn mirror_template_uses_tag_and_version() {
        let m = "https://registry.npmmirror.com/-/binary/git-for-windows/{tag}/Git-{version}-64-bit.exe";
        assert_eq!(
            super::super::fill_url_template(m, "2.55.0", "v2.55.0.windows.1"),
            "https://registry.npmmirror.com/-/binary/git-for-windows/v2.55.0.windows.1/Git-2.55.0-64-bit.exe"
        );
    }
}
