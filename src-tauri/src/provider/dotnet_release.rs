//! .NET SDK 版本源：`releases-index.json` 给出各版本通道与其最新 SDK。
//! 只保留仍受支持的通道（active / maintenance），排除 go-live（预览 / RC）。
//! 下载地址在 resolve 时读该通道的 `releases.json` 精确定位（地址含 SDK 版本号，缓存不冲突）。
//! 官方无国内镜像，安装包走 builds.dotnet.microsoft.com。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const INDEX: &str = "https://dotnetcli.blob.core.windows.net/dotnet/release-metadata/releases-index.json";
const WIN_X64_EXE: &str = "dotnet-sdk-win-x64.exe";

pub struct DotnetReleaseProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::DotnetRelease { prefer }) => *prefer,
        _ => Prefer::Lts,
    }
}

#[derive(serde::Deserialize)]
struct RawIndex {
    #[serde(rename = "releases-index", default)]
    channels: Vec<RawChannel>,
}

#[derive(serde::Deserialize)]
struct RawChannel {
    #[serde(rename = "latest-sdk", default)]
    latest_sdk: String,
    #[serde(rename = "release-type", default)]
    release_type: String,
    #[serde(rename = "support-phase", default)]
    support_phase: String,
}

/// 纯解析：每通道一条（取该通道最新 SDK），只保留 active / maintenance。
pub fn parse_index(json: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let idx: RawIndex = serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let mut out: Vec<VersionInfo> = idx
        .channels
        .into_iter()
        .filter(|c| matches!(c.support_phase.as_str(), "active" | "maintenance"))
        .filter(|c| !c.latest_sdk.trim().is_empty())
        .map(|c| VersionInfo {
            version: c.latest_sdk,
            lts: c.release_type == "lts",
            lts_name: None,
            date: None,
            recommended: false,
        })
        .collect();
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

/// SDK 版本 → 所属通道：`10.0.401` → `10.0`。
pub fn channel_of(sdk_version: &str) -> String {
    let mut parts = sdk_version.split('.');
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        _ => sdk_version.to_string(),
    }
}

pub fn channel_releases_url(channel: &str) -> String {
    format!("https://builds.dotnet.microsoft.com/dotnet/release-metadata/{channel}/releases.json")
}

#[derive(serde::Deserialize)]
struct RawReleases {
    #[serde(default)]
    releases: Vec<RawRelease>,
}

#[derive(serde::Deserialize)]
struct RawRelease {
    #[serde(default)]
    sdks: Vec<RawSdk>,
}

#[derive(serde::Deserialize)]
struct RawSdk {
    version: String,
    #[serde(default)]
    files: Vec<RawFile>,
}

#[derive(serde::Deserialize)]
struct RawFile {
    name: String,
    url: String,
}

/// 在通道的 releases.json 里定位目标 SDK 版本的 win-x64 安装包地址。
pub fn find_win_x64_exe(json: &str, sdk_version: &str) -> Option<String> {
    let rel: RawReleases = serde_json::from_str(json).ok()?;
    for r in rel.releases {
        for s in r.sdks {
            if s.version != sdk_version {
                continue;
            }
            let hit = s
                .files
                .iter()
                .find(|f| f.name == WIN_X64_EXE)
                .or_else(|| s.files.iter().find(|f| f.name.ends_with("win-x64.exe")))?;
            return Some(hit.url.clone());
        }
    }
    None
}

#[async_trait::async_trait]
impl VersionProvider for DotnetReleaseProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text(INDEX).await?;
        let list = parse_index(&body, prefer)?;
        // 索引结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index("没有解析到仍受支持的 .NET 版本线".into()));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let body = crate::http::get_text(&channel_releases_url(&channel_of(version))).await?;
        let url = find_win_x64_exe(&body, version).ok_or_else(|| ProviderError::NoAsset(version.to_string()))?;
        let file_name = url.rsplit('/').next().unwrap_or(WIN_X64_EXE).to_string();
        Ok(vec![DownloadTarget { file_name, urls: vec![url], sha256: None, md5: None }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 线上实测形态：含 go-live（预览）、active、maintenance、eol 四类通道。
    const INDEX_JSON: &str = r#"{"releases-index":[
      {"channel-version":"11.0","latest-sdk":"11.0.100-rc.1.26425.128","release-type":"sts","support-phase":"go-live"},
      {"channel-version":"10.0","latest-sdk":"10.0.401","release-type":"lts","support-phase":"active"},
      {"channel-version":"9.0","latest-sdk":"9.0.305","release-type":"sts","support-phase":"maintenance"},
      {"channel-version":"7.0","latest-sdk":"7.0.410","release-type":"sts","support-phase":"eol"}
    ]}"#;

    const RELEASES_JSON: &str = r#"{"releases":[
      {"release-version":"10.0.12","sdks":[
        {"version":"10.0.112","files":[{"name":"dotnet-sdk-win-x64.exe","url":"https://builds.dotnet.microsoft.com/dotnet/Sdk/10.0.112/dotnet-sdk-10.0.112-win-x64.exe"}]},
        {"version":"10.0.401","files":[
          {"name":"dotnet-sdk-linux-x64.tar.gz","url":"https://builds.dotnet.microsoft.com/dotnet/Sdk/10.0.401/dotnet-sdk-linux-x64.tar.gz"},
          {"name":"dotnet-sdk-win-x64.exe","url":"https://builds.dotnet.microsoft.com/dotnet/Sdk/10.0.401/dotnet-sdk-10.0.401-win-x64.exe"}
        ]}
      ]}
    ]}"#;

    #[test]
    fn keeps_supported_channels_and_marks_lts() {
        let list = parse_index(INDEX_JSON, Prefer::Lts).unwrap();
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["10.0.401", "9.0.305"],
            "go-live 与 eol 通道都应被排除"
        );
        assert!(list.iter().find(|v| v.version == "10.0.401").unwrap().lts);
        assert!(!list.iter().find(|v| v.version == "9.0.305").unwrap().lts);
    }

    #[test]
    fn recommended_is_highest_lts() {
        let list = parse_index(INDEX_JSON, Prefer::Lts).unwrap();
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "10.0.401");
    }

    #[test]
    fn channel_is_derived_from_sdk_version() {
        assert_eq!(channel_of("10.0.401"), "10.0");
        assert_eq!(channel_of("8.0"), "8.0");
        assert_eq!(channel_of("weird"), "weird");
        assert!(channel_releases_url("10.0").ends_with("/10.0/releases.json"));
    }

    #[test]
    fn finds_the_exact_sdk_win_x64_exe() {
        let url = find_win_x64_exe(RELEASES_JSON, "10.0.401").unwrap();
        assert_eq!(url, "https://builds.dotnet.microsoft.com/dotnet/Sdk/10.0.401/dotnet-sdk-10.0.401-win-x64.exe");
        assert!(find_win_x64_exe(RELEASES_JSON, "10.0.999").is_none());
        assert!(find_win_x64_exe("not json", "10.0.401").is_none());
    }

    #[test]
    fn malformed_index_is_error_and_empty_shape_is_graceful() {
        assert!(parse_index("not json", Prefer::Lts).is_err());
        assert!(parse_index("[]", Prefer::Lts).unwrap().is_empty());
    }
}
