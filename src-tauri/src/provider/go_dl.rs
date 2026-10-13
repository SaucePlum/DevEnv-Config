//! Go 版本源：解析 `https://go.dev/dl/?mode=json&include=all`。
//! 只保留 stable 通道（排除 beta / rc）。
//! 镜像：阿里云 `https://mirrors.aliyun.com/golang/`（实测可用），官网 CDN 之后回落。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const INDEX: &str = "https://go.dev/dl/?mode=json&include=all";

pub struct GoDlProvider;

#[derive(serde::Deserialize)]
struct RawRelease {
    version: String,
    #[serde(default)]
    stable: bool,
}

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::GoDl { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

/// `go1.27.2` → `1.27.2`。
/// 去掉 `go` 前缀，否则公共的 parse_version 会把 `go1` 当成非数字段，整串退化为空、
/// 导致排序与「推荐版本」判定全部失效。
pub fn strip_go_prefix(v: &str) -> String {
    v.trim().trim_start_matches("go").to_string()
}

/// 纯解析：过滤出 stable 版本，降序排列并标记 recommended。
pub fn parse_index(json: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let raw: Vec<RawRelease> =
        serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let mut out: Vec<VersionInfo> = raw
        .into_iter()
        .filter(|r| r.stable)
        .map(|r| VersionInfo {
            version: strip_go_prefix(&r.version),
            lts: false,
            lts_name: None,
            date: None,
            recommended: false,
        })
        .collect();
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

/// Windows x64 安装包文件名：`1.27.2` → `go1.27.2.windows-amd64.msi`
pub fn msi_name(version: &str) -> String {
    format!("go{}.windows-amd64.msi", strip_go_prefix(version))
}

/// 安装包地址：官网 → Google CDN → 阿里云镜像。
pub fn installer_urls(version: &str) -> Vec<String> {
    let f = msi_name(version);
    vec![
        format!("https://go.dev/dl/{f}"),
        format!("https://dl.google.com/go/{f}"),
        format!("https://mirrors.aliyun.com/golang/{f}"),
    ]
}

#[async_trait::async_trait]
impl VersionProvider for GoDlProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text(INDEX).await?;
        let list = parse_index(&body, prefer)?;
        // 索引结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index("没有解析到 Go 的正式版本".into()));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        Ok(vec![DownloadTarget {
            file_name: msi_name(version),
            urls: installer_urls(version),
            sha256: None,
            md5: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"version":"go1.27.2","stable":true},
      {"version":"go1.28.0rc1","stable":false},
      {"version":"go1.26.9","stable":true}
    ]"#;

    #[test]
    fn keeps_stable_only_and_sorts_desc() {
        let list = parse_index(SAMPLE, Prefer::Latest).unwrap();
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["1.27.2", "1.26.9"]
        );
        assert!(!list.iter().any(|v| v.version.contains("rc")), "rc 不应出现");
    }

    #[test]
    fn recommended_is_highest_stable() {
        // 关键回归：`go` 前缀若未剥离，版本会退化成空串，推荐项会错落到 1.26.9
        let list = parse_index(SAMPLE, Prefer::Latest).unwrap();
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "1.27.2");
    }

    #[test]
    fn empty_and_malformed_inputs() {
        assert!(parse_index("[]", Prefer::Latest).unwrap().is_empty());
        assert!(parse_index("not json", Prefer::Latest).is_err());
    }

    #[test]
    fn installer_urls_put_official_first_then_cdn_then_mirror() {
        let urls = installer_urls("1.27.2");
        assert_eq!(urls.len(), 3);
        assert_eq!(urls[0], "https://go.dev/dl/go1.27.2.windows-amd64.msi");
        assert_eq!(urls[1], "https://dl.google.com/go/go1.27.2.windows-amd64.msi");
        assert_eq!(urls[2], "https://mirrors.aliyun.com/golang/go1.27.2.windows-amd64.msi");
        // 传入带前缀的写法也应得到同一个文件名
        assert_eq!(msi_name("go1.27.2"), "go1.27.2.windows-amd64.msi");
    }

    #[tokio::test]
    async fn resolve_uses_manifest_item() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("go").unwrap().clone();
        let t = GoDlProvider.resolve(&item, "1.27.2").await.unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file_name, "go1.27.2.windows-amd64.msi");
        assert_eq!(t[0].urls[0], "https://go.dev/dl/go1.27.2.windows-amd64.msi");
    }
}
