//! Node.js 版本源：解析 `https://nodejs.org/dist/index.json`。
//! 镜像：npmmirror（`https://registry.npmmirror.com/-/binary/node/`）。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

pub struct NodeDistProvider;

#[derive(serde::Deserialize)]
struct RawEntry {
    version: String,
    #[serde(default)]
    lts: serde_json::Value,
    #[serde(default)]
    date: String,
}

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::NodeDist { prefer }) => *prefer,
        _ => Prefer::Lts,
    }
}

/// 纯解析：把 dist/index.json 内容转为版本列表（已排序并标记 recommended）。
pub fn parse_index(json: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let raw: Vec<RawEntry> =
        serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let mut out: Vec<VersionInfo> = raw
        .into_iter()
        .map(|e| {
            let (lts, lts_name) = match &e.lts {
                serde_json::Value::String(s) => (true, Some(s.clone())),
                _ => (false, None),
            };
            VersionInfo {
                version: e.version,
                lts,
                lts_name,
                date: if e.date.is_empty() { None } else { Some(e.date) },
                recommended: false,
            }
        })
        .collect();
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

#[async_trait::async_trait]
impl VersionProvider for NodeDistProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text("https://nodejs.org/dist/index.json").await?;
        parse_index(&body, prefer)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let file = format!("node-{version}-x64.msi");
        let urls = vec![
            format!("https://nodejs.org/dist/{version}/{file}"),
            format!("https://registry.npmmirror.com/-/binary/node/{version}/{file}"),
        ];
        Ok(vec![DownloadTarget { file_name: file, urls, sha256: None, md5: None }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"version":"v24.21.0","date":"2026-08-01","lts":"Krypton","files":["win-x64-msi"]},
      {"version":"v25.0.0","date":"2026-09-01","lts":false,"files":["win-x64-msi"]},
      {"version":"v22.20.0","date":"2026-07-01","lts":"Jod","files":["win-x64-msi"]}
    ]"#;

    #[test]
    fn parses_versions_and_lts_flag() {
        let list = parse_index(SAMPLE, Prefer::Lts).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].version, "v25.0.0");
        assert!(!list[0].lts);
        assert!(list.iter().any(|v| v.version == "v24.21.0" && v.lts));
    }

    #[test]
    fn picks_highest_lts_as_recommended() {
        let list = parse_index(SAMPLE, Prefer::Lts).unwrap();
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "v24.21.0");
    }

    #[test]
    fn empty_index_yields_empty_list() {
        assert!(parse_index("[]", Prefer::Lts).unwrap().is_empty());
    }

    #[test]
    fn malformed_index_is_error() {
        assert!(parse_index("{}", Prefer::Lts).is_err());
    }

    #[test]
    fn missing_lts_field_defaults_to_non_lts() {
        let json = r#"[{"version":"v20.0.0","date":"2023-01-01"}]"#;
        let list = parse_index(json, Prefer::Lts).unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].lts);
    }

    #[tokio::test]
    async fn resolve_builds_official_and_mirror_urls() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("node").unwrap().clone();
        let t = NodeDistProvider.resolve(&item, "v24.21.0").await.unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file_name, "node-v24.21.0-x64.msi");
        assert_eq!(t[0].urls[0], "https://nodejs.org/dist/v24.21.0/node-v24.21.0-x64.msi");
        assert_eq!(
            t[0].urls[1],
            "https://registry.npmmirror.com/-/binary/node/v24.21.0/node-v24.21.0-x64.msi"
        );
    }
}
