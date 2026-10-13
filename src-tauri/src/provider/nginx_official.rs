//! nginx 版本源：抓取 `https://nginx.org/en/download.html`。
//! 页面同时列出主线版（奇数次版本号）与稳定版（偶数次版本号），
//! 稳定线视为 LTS 语义，供「仅看 LTS」开关与推荐位使用。
//! 镜像：华为云 `https://mirrors.huaweicloud.com/nginx/`（实测可用；阿里云 404 已排除）。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const PAGE: &str = "https://nginx.org/en/download.html";

pub struct NginxOfficialProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::NginxOfficial { prefer }) => *prefer,
        _ => Prefer::Lts,
    }
}

/// 次版本号为偶数即稳定线（nginx 官方节奏：奇数 = mainline，偶数 = stable）。
pub fn is_stable(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.get(1).and_then(|m| m.parse::<u64>().ok()).map(|m| m % 2 == 0).unwrap_or(false)
}

/// 纯解析：从下载页 HTML 中提取版本号，去重后按稳定线标记 LTS。
pub fn parse_page(html: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let re = regex::Regex::new(r#"nginx-(\d+\.\d+\.\d+)\.zip"#).expect("合法正则");
    let mut out: Vec<VersionInfo> = re
        .captures_iter(html)
        .map(|cap| VersionInfo {
            version: cap[1].to_string(),
            lts: is_stable(&cap[1]),
            lts_name: None,
            date: None,
            recommended: false,
        })
        .collect();
    out.sort_by(|a, b| super::cmp_version(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

#[async_trait::async_trait]
impl VersionProvider for NginxOfficialProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text(PAGE).await?;
        let list = parse_page(&body, prefer)?;
        // 页面结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index(crate::tr!("没有从下载页解析到 nginx 版本", "No nginx version parsed from the download page")));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let file = format!("nginx-{version}.zip");
        Ok(vec![DownloadTarget {
            file_name: file.clone(),
            urls: vec![
                format!("https://nginx.org/download/{file}"),
                format!("https://mirrors.huaweicloud.com/nginx/{file}"),
            ],
            sha256: None,
            md5: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"
      <td>nginx-1.31.6.zip</td>
      <a href="/download/nginx-1.30.5.zip">nginx-1.30.5</a>
      <a href="/download/nginx-1.28.3.zip.gz">gz 引用里的版本也应计入</a>
      <a href="/download/nginx-1.30.5.zip">重复链接</a>
      <a href="/download/nginx-1.26.3.zip">pem key</a>
    "#;

    #[test]
    fn stable_line_marks_lts() {
        assert!(is_stable("1.30.5"), "偶数次版本号是稳定线");
        assert!(!is_stable("1.31.6"), "奇数次版本号是主线版");
        assert!(!is_stable("abc"));
    }

    #[test]
    fn parses_dedupes_and_sorts() {
        let list = parse_page(HTML, Prefer::Lts).unwrap();
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["1.31.6", "1.30.5", "1.28.3", "1.26.3"],
            "重复链接去重、降序排列"
        );
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "1.30.5");
        assert!(list.iter().find(|v| v.version == "1.30.5").unwrap().lts);
        assert!(!list.iter().find(|v| v.version == "1.31.6").unwrap().lts);
    }

    #[test]
    fn empty_and_malformed_inputs() {
        assert!(parse_page("<html></html>", Prefer::Lts).unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolve_builds_official_and_mirror_urls() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("nginx").unwrap().clone();
        let t = NginxOfficialProvider.resolve(&item, "1.30.5").await.unwrap();
        assert_eq!(t[0].file_name, "nginx-1.30.5.zip");
        assert_eq!(t[0].urls[0], "https://nginx.org/download/nginx-1.30.5.zip");
        assert_eq!(t[0].urls[1], "https://mirrors.huaweicloud.com/nginx/nginx-1.30.5.zip");
    }
}
