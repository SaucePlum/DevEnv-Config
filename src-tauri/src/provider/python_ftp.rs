//! Python 版本源：解析 `https://www.python.org/ftp/python/` 的目录列表。
//! 镜像：华为云 `https://mirrors.huaweicloud.com/python/`。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

pub struct PythonFtpProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::PythonFtp { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

/// 纯解析：从 HTML 目录列表里提取形如 `3.12.7/` 的版本目录。
pub fn parse_directory(html: &str, prefer: Prefer) -> Vec<VersionInfo> {
    let re = regex::Regex::new(r#"href="(\d+\.\d+\.\d+)/""#).expect("合法正则");
    let mut out = Vec::new();
    for cap in re.captures_iter(html) {
        let version = cap[1].to_string();
        out.push(VersionInfo {
            version,
            lts: false,
            lts_name: None,
            date: None,
            recommended: false,
        });
    }
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out.dedup_by(|a, b| a.version == b.version);
    sort_and_mark(&mut out, prefer);
    out
}

#[async_trait::async_trait]
impl VersionProvider for PythonFtpProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text("https://www.python.org/ftp/python/").await?;
        Ok(parse_directory(&body, prefer))
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let file = format!("python-{version}-amd64.exe");
        let urls = vec![
            format!("https://www.python.org/ftp/python/{version}/{file}"),
            format!("https://mirrors.huaweicloud.com/python/{version}/{file}"),
        ];
        Ok(vec![DownloadTarget { file_name: file, urls, sha256: None, md5: None }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
      <a href="3.14.7/">3.14.7/</a>
      <a href="3.13.9/">3.13.9/</a>
      <a href="2.7.18/">2.7.18/</a>
      <a href="../">../</a>
    "#;

    #[test]
    fn extracts_version_directories_only() {
        let list = parse_directory(SAMPLE, Prefer::Latest);
        let names: Vec<&str> = list.iter().map(|v| v.version.as_str()).collect();
        assert!(names.contains(&"3.14.7"));
        assert!(names.contains(&"3.13.9"));
        assert!(names.contains(&"2.7.18"));
        assert_eq!(list.len(), 3, "`../` 不应被当成版本");
    }

    #[test]
    fn recommended_is_latest() {
        let list = parse_directory(SAMPLE, Prefer::Latest);
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "3.14.7");
    }

    #[test]
    fn deduplicates_repeated_links() {
        let dup = r#"<a href="3.12.1/">a</a><a href="3.12.1/">b</a>"#;
        assert_eq!(parse_directory(dup, Prefer::Latest).len(), 1);
    }

    #[test]
    fn empty_html_yields_empty_list() {
        assert!(parse_directory("<html></html>", Prefer::Latest).is_empty());
    }
}
