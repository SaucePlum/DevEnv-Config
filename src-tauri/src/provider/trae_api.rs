//! Trae 版本 API 适配器：`https://api.trae.com.cn/icube/api/v1/native/version/trae/latest`
//! 返回 `data.manifest.win32.versions[]`（按 region 区分 cn / sg / va / usttp）。
//! 国内版取 region=cn，国际版取 region=sg；列表只暴露「最新」，resolve 重新请求拿真实直链。

use super::{DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, ProviderDef};

const API: &str = "https://api.trae.com.cn/icube/api/v1/native/version/trae/latest";

pub struct TraeApiProvider;

fn region_of(item: &Item) -> Result<String, ProviderError> {
    match item.provider.as_ref() {
        Some(ProviderDef::TraeApi { region }) => Ok(region.clone()),
        _ => Err(ProviderError::NoProvider),
    }
}

#[derive(serde::Deserialize)]
struct RawResp {
    #[serde(default)]
    data: RawData,
}

#[derive(Default, serde::Deserialize)]
struct RawData {
    #[serde(default)]
    manifest: RawManifest,
}

#[derive(Default, serde::Deserialize)]
struct RawManifest {
    #[serde(default, rename = "win32")]
    win32: RawPlatform,
}

#[derive(Default, serde::Deserialize)]
struct RawPlatform {
    #[serde(default)]
    versions: Vec<RawVersion>,
}

#[derive(serde::Deserialize)]
struct RawVersion {
    #[serde(default)]
    region: String,
    #[serde(default)]
    arch: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    version: String,
}

/// 纯解析：按 region + x64 过滤出 Windows 安装包。
pub fn pick(json: &str, region: &str) -> Result<(String, String), ProviderError> {
    let r: RawResp = serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    r.data
        .manifest
        .win32
        .versions
        .iter()
        .find(|v| v.region == region && v.arch == "x64" && !v.url.is_empty())
        .map(|v| (v.version.clone(), v.url.clone()))
        .ok_or_else(|| ProviderError::NoAsset(region.to_string()))
}

#[async_trait::async_trait]
impl VersionProvider for TraeApiProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let region = region_of(item)?;
        let body = crate::http::get_text(API).await?;
        let (version, _) = pick(&body, &region)?;
        Ok(vec![VersionInfo { version, lts: false, lts_name: None, date: None, recommended: true }])
    }

    async fn resolve(&self, item: &Item, _version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let region = region_of(item)?;
        let body = crate::http::get_text(API).await?;
        let (_, url) = pick(&body, &region)?;
        let file_name = url.rsplit('/').next().unwrap_or("Trae-Setup-x64.exe").to_string();
        Ok(vec![DownloadTarget { file_name, urls: vec![url], sha256: None, md5: None }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 线上实测形态（截取）。
    const JSON: &str = r#"{"success":true,"data":{"manifest":{"win32":{
      "download":[{"region":"cn","x64":"https://cn.example.com/Trae-Setup-x64.exe"}],
      "versions":[
        {"region":"cn","arch":"x64","url":"https://cn.example.com/Trae-Setup-x64.exe","version":"3.5.20"},
        {"region":"sg","arch":"x64","url":"https://sg.example.com/Trae-Setup-x64.exe","version":"3.5.20"},
        {"region":"sg","arch":"apple","url":"https://sg.example.com/mac.dmg","version":"3.5.20"}
      ]}}}}"#;

    #[test]
    fn picks_region_and_x64() {
        let (v, u) = pick(JSON, "cn").unwrap();
        assert_eq!(v, "3.5.20");
        assert!(u.contains("cn.example.com"));
        let (_, u2) = pick(JSON, "sg").unwrap();
        assert!(u2.contains("sg.example.com"));
        assert!(pick(JSON, "nope").is_err(), "未知 region 应报无资产");
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(pick("not json", "cn").is_err());
        assert!(pick("{}", "cn").is_err());
    }

    #[tokio::test]
    async fn resolve_uses_manifest_items() {
        let m = crate::manifest::Manifest::load().unwrap();
        for id in ["trae-cn", "trae-intl"] {
            let item = m.get(id).unwrap().clone();
            let t = TraeApiProvider.resolve(&item, "最新").await.unwrap();
            assert!(t[0].file_name.starts_with("Trae-Setup"), "{id} 文件名异常");
            assert!(t[0].urls[0].contains("trae"), "{id} 直链异常: {}", t[0].urls[0]);
        }
        // 国内项必须走国内 CDN，国际项必须走国际 CDN
        let cn = TraeApiProvider.resolve(&m.get("trae-cn").unwrap().clone(), "最新").await.unwrap();
        assert!(cn[0].urls[0].contains("trae.com.cn"), "国内版应走国内 CDN");
        let intl = TraeApiProvider.resolve(&m.get("trae-intl").unwrap().clone(), "最新").await.unwrap();
        assert!(intl[0].urls[0].contains("trae.ai"), "国际版应走国际 CDN");
    }
}
