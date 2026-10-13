//! 平铺版本 API 适配器：GET 一次返回 `{version, url, sha256hash}` 的接口
//! （腾讯 CodeBuddy / WorkBuddy 的 v2/update 端点等）。
//! 单版本项：列表只暴露「最新」，resolve 时重新请求拿真实直链与校验和，
//! 避免 hardcode 版本化 URL 过期。

use super::{DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, ProviderDef};

pub struct UpdateApiProvider;

fn cfg(item: &Item) -> Result<(String, String, String, String), ProviderError> {
    match item.provider.as_ref() {
        Some(ProviderDef::UpdateApi { url, version_field, url_field, sha_field }) => Ok((
            url.clone(),
            version_field.clone().unwrap_or_else(|| "version".into()),
            url_field.clone().unwrap_or_else(|| "url".into()),
            sha_field.clone().unwrap_or_else(|| "sha256hash".into()),
        )),
        _ => Err(ProviderError::NoProvider),
    }
}

#[derive(serde::Deserialize)]
struct RawResp {
    #[serde(default)]
    version: serde_json::Value,
    #[serde(default)]
    url: serde_json::Value,
    #[serde(default)]
    sha256hash: serde_json::Value,
}

/// 字段名可配置：按实际字段名取字符串值。
fn field(r: &RawResp, name: &str) -> Option<String> {
    let v = match name {
        "version" => &r.version,
        "url" => &r.url,
        "sha256hash" => &r.sha256hash,
        _ => return None,
    };
    v.as_str().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// 纯解析：从平铺 JSON 提取 (版本, 直链, sha256)。
pub fn parse_resp(
    json: &str,
    version_field: &str,
    url_field: &str,
    sha_field: &str,
) -> Result<(String, String, Option<String>), ProviderError> {
    let r: RawResp = serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let version = field(&r, version_field).ok_or_else(|| ProviderError::Parse(format!("缺少字段 {version_field}")))?;
    let url = field(&r, url_field).ok_or_else(|| ProviderError::Parse(format!("缺少字段 {url_field}")))?;
    let sha = field(&r, sha_field);
    Ok((version, url, sha))
}

#[async_trait::async_trait]
impl VersionProvider for UpdateApiProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let (url, vf, _, _) = cfg(item)?;
        let body = crate::http::get_text(&url).await?;
        let (version, _, _) = parse_resp(&body, &vf, "url", "sha256hash")?;
        Ok(vec![VersionInfo { version, lts: false, lts_name: None, date: None, recommended: true }])
    }

    async fn resolve(&self, item: &Item, _version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let (url, vf, uf, sf) = cfg(item)?;
        let body = crate::http::get_text(&url).await?;
        let (version, real_url, sha) = parse_resp(&body, &vf, &uf, &sf)?;
        let file_name = real_url.rsplit('/').next().unwrap_or("download.exe").to_string();
        Ok(vec![DownloadTarget {
            file_name: if file_name.contains(&version) { file_name } else { format!("{version}-{file_name}") },
            urls: vec![real_url],
            sha256: sha.filter(|s| !s.is_empty()),
            md5: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{"version":"4.12.1.39217423","url":"https://cos.example.com/CodeBuddy-4.12.1.exe","productVersion":"x","sha256hash":"abc123","timestamp":1,"hash":"","name":""}"#;

    #[test]
    fn parses_flat_response() {
        let (v, u, sha) = parse_resp(JSON, "version", "url", "sha256hash").unwrap();
        assert_eq!(v, "4.12.1.39217423");
        assert_eq!(u, "https://cos.example.com/CodeBuddy-4.12.1.exe");
        assert_eq!(sha.as_deref(), Some("abc123"));
    }

    #[test]
    fn missing_fields_are_errors() {
        assert!(parse_resp("{}", "version", "url", "sha256hash").is_err());
        assert!(parse_resp("not json", "version", "url", "sha256hash").is_err());
    }

    #[tokio::test]
    async fn resolve_uses_manifest_item() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("codebuddy-cn").unwrap().clone();
        let t = UpdateApiProvider.resolve(&item, "最新").await.unwrap();
        assert!(t[0].urls[0].contains("codebuddy"), "应解析出真实直链：{}", t[0].urls[0]);
        assert!(!t[0].file_name.is_empty());
    }
}
