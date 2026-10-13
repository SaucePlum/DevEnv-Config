//! Eclipse Temurin（OpenJDK）版本源：`api.adoptium.net`。
//! 只暴露「特性版本」（8 / 11 / 17 / 21 / 25 …），下载时解析该版本最新的 GA 构建。
//! 安装包地址：GitHub Release → 清华 Adoptium 镜像（实测可用）。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const INFO: &str = "https://api.adoptium.net/v3/info/available_releases";

pub struct AdoptiumProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::Adoptium { prefer }) => *prefer,
        _ => Prefer::Lts,
    }
}

#[derive(serde::Deserialize)]
struct RawInfo {
    #[serde(default)]
    available_releases: Vec<u32>,
    #[serde(default)]
    available_lts_releases: Vec<u32>,
}

/// 纯解析：把特性版本列表转成 VersionInfo，LTS 取自接口的 available_lts_releases。
pub fn parse_info(json: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let info: RawInfo = serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let lts = info.available_lts_releases;
    let mut out: Vec<VersionInfo> = info
        .available_releases
        .into_iter()
        .map(|m| VersionInfo {
            version: m.to_string(),
            lts: lts.contains(&m),
            lts_name: None,
            date: None,
            recommended: false,
        })
        .collect();
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

/// 某特性版本最新 GA 构建的查询地址。
pub fn latest_asset_url(major: &str) -> String {
    format!(
        concat!(
            "https://api.adoptium.net/v3/assets/feature_releases/{}/ga",
            "?os=windows&architecture=x64&image_type=jdk&page_size=1"
        ),
        major
    )
}

#[derive(serde::Deserialize)]
struct RawAsset {
    #[serde(default)]
    binaries: Vec<RawBinary>,
}

#[derive(serde::Deserialize)]
struct RawBinary {
    #[serde(default)]
    architecture: String,
    #[serde(default)]
    image_type: String,
    #[serde(default)]
    os: String,
    package: RawPackage,
}

#[derive(serde::Deserialize)]
struct RawPackage {
    name: String,
    link: String,
    #[serde(default)]
    checksum: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssetRef {
    pub name: String,
    pub link: String,
    pub checksum: Option<String>,
}

/// 从 assets 响应里挑出 windows / x64 / jdk 的压缩包。
pub fn pick_windows_x64_jdk(json: &str) -> Option<AssetRef> {
    let assets: Vec<RawAsset> = serde_json::from_str(json).ok()?;
    for a in assets {
        for b in a.binaries {
            if b.os.eq_ignore_ascii_case("windows")
                && b.architecture.eq_ignore_ascii_case("x64")
                && b.image_type.eq_ignore_ascii_case("jdk")
            {
                return Some(AssetRef {
                    name: b.package.name,
                    link: b.package.link,
                    checksum: b.package.checksum,
                });
            }
        }
    }
    None
}

/// 清华 Adoptium 镜像地址，目录结构为 `{major}/jdk/x64/windows/{文件名}`。
pub fn mirror_url(major: &str, file_name: &str) -> String {
    format!("https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{major}/jdk/x64/windows/{file_name}")
}

#[async_trait::async_trait]
impl VersionProvider for AdoptiumProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text(INFO).await?;
        let list = parse_info(&body, prefer)?;
        // 索引结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index(crate::tr!("没有解析到可用的 JDK 版本", "No usable JDK version parsed")));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let body = crate::http::get_text(&latest_asset_url(version)).await?;
        let a = pick_windows_x64_jdk(&body).ok_or_else(|| ProviderError::NoAsset(version.to_string()))?;
        let urls = vec![a.link.clone(), mirror_url(version, &a.name)];
        Ok(vec![DownloadTarget { file_name: a.name, urls, sha256: a.checksum, md5: None }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 线上实测形态（截取部分字段）。
    const INFO_JSON: &str = r#"{
      "available_releases":[8,11,17,21,25,26],
      "available_lts_releases":[8,11,17,21,25],
      "most_recent_lts":25
    }"#;

    const ASSETS_JSON: &str = r#"[{
      "release_name":"jdk-21.0.12.1+1",
      "binaries":[
        {"architecture":"x64","image_type":"jre","os":"windows","package":{"name":"OpenJDK21U-jre_x64_windows_hotspot_21.0.12.1_1.zip","link":"https://github.com/x/jre.zip"}},
        {"architecture":"x64","image_type":"jdk","os":"windows","package":{"name":"OpenJDK21U-jdk_x64_windows_hotspot_21.0.12.1_1.zip","link":"https://github.com/x/jdk.zip","checksum":"abc123"}},
        {"architecture":"aarch64","image_type":"jdk","os":"windows","package":{"name":"OpenJDK21U-jdk_aarch64_windows_hotspot_21.0.12.1_1.zip","link":"https://github.com/x/arm.zip"}}
      ]
    }]"#;

    #[test]
    fn lists_feature_releases_with_lts_flags() {
        let list = parse_info(INFO_JSON, Prefer::Lts).unwrap();
        assert_eq!(list.len(), 6);
        assert!(list.iter().find(|v| v.version == "21").unwrap().lts);
        assert!(!list.iter().find(|v| v.version == "26").unwrap().lts);
    }

    #[test]
    fn recommended_is_highest_lts() {
        let list = parse_info(INFO_JSON, Prefer::Lts).unwrap();
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "25");
    }

    #[test]
    fn picks_windows_x64_jdk_only() {
        let a = pick_windows_x64_jdk(ASSETS_JSON).unwrap();
        assert_eq!(a.name, "OpenJDK21U-jdk_x64_windows_hotspot_21.0.12.1_1.zip");
        assert_eq!(a.link, "https://github.com/x/jdk.zip");
        assert_eq!(a.checksum.as_deref(), Some("abc123"));
        assert!(pick_windows_x64_jdk("[]").is_none());
        assert!(pick_windows_x64_jdk("not json").is_none());
    }

    #[test]
    fn asset_url_and_mirror_url_are_well_formed() {
        assert!(latest_asset_url("21").contains("/v3/assets/feature_releases/21/ga"));
        assert!(latest_asset_url("21").contains("image_type=jdk"));
        assert_eq!(
            mirror_url("21", "OpenJDK21U-jdk_x64_windows_hotspot_21.0.12.1_1.zip"),
            "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/x64/windows/OpenJDK21U-jdk_x64_windows_hotspot_21.0.12.1_1.zip"
        );
    }
}
