//! Apache HTTP Server 版本源：Apache Lounge 下载页抓取。
//! Windows 官方不分发二进制，Apache Lounge 是官方推荐的第三方构建站。
//! 主页列出当前 VS18 构建（文件名带构建号，如 httpd-2.4.69-261002-Win64-VS18.zip），
//! VS16 存档页还保留旧版。文件名含构建号，无法从版本号反推，resolve 时重新抓页面取完整链接。
//! 实测（2026-10）：页面上列出的直链可下（application/zip）；不在页面上的旧直链
//! （如 VS17）会被替换为 HTML 提示页，故只收录页面上确实出现的链接。
//! 无国内镜像（官方 Apache 镜像仅源码），单源风险已接受。
//! VS17+ 构建依赖 VC++ 2022 运行库（清单 vcredist 项）。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const BASE: &str = "https://www.apachelounge.com";
const PAGES: [&str; 2] = [
    "https://www.apachelounge.com/download/",
    "https://www.apachelounge.com/download/VS16/",
];

pub struct ApacheLoungeProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::ApacheLounge { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

/// (版本号, 完整直链) 对。
#[derive(Debug, Clone)]
pub struct ApacheZip {
    pub version: String,
    pub url: String,
}

/// 从一段页面 HTML 中提取 Win64 zip 链接（大小写不敏感：VS18 用 Win64、VS16 用 win64）。
pub fn parse_page(html: &str) -> Vec<ApacheZip> {
    let re = regex::Regex::new(
        r#"(?i)href="(/download/VS\d+/binaries/httpd-(\d+\.\d+\.\d+)(?:-\d+)?-win64-vs\d+\.zip)""#,
    )
    .expect("合法正则");
    re.captures_iter(html)
        .map(|cap| ApacheZip {
            version: cap[2].to_string(),
            url: format!("{BASE}{}", &cap[1]),
        })
        .collect()
}

/// 纯解析：合并各页面的 zip，去重并排序。
pub fn merge_pages(pages: &[&str], prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let mut out: Vec<VersionInfo> = pages
        .iter()
        .flat_map(|html| parse_page(html))
        .map(|z| VersionInfo {
            version: z.version,
            lts: false,
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

/// 依次抓取各页面 HTML。
async fn fetch_pages() -> Result<Vec<String>, ProviderError> {
    let mut out = Vec::new();
    let mut last: Option<ProviderError> = None;
    for p in PAGES {
        match crate::http::get_text(p).await {
            Ok(body) => out.push(body),
            Err(e) => last = Some(e.into()),
        }
    }
    // 至少一个页面抓到即可继续；全部失败才报错
    if out.is_empty() {
        Err(last.unwrap_or(ProviderError::Index("Apache 页面地址为空".into())))
    } else {
        Ok(out)
    }
}

#[async_trait::async_trait]
impl VersionProvider for ApacheLoungeProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let pages = fetch_pages().await?;
        let list = merge_pages(&pages.iter().map(|s| s.as_str()).collect::<Vec<_>>(), prefer)?;
        // 页面结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index("没有从下载页解析到 Apache 的 Win64 包".into()));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let pages = fetch_pages().await?;
        for html in &pages {
            for z in parse_page(html) {
                if z.version == version {
                    let file_name = z.url.rsplit('/').next().unwrap_or("httpd.zip").to_string();
                    return Ok(vec![DownloadTarget { file_name, urls: vec![z.url], sha256: None, md5: None }]);
                }
            }
        }
        Err(ProviderError::NoAsset(version.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN_PAGE: &str = r#"
      <a href="/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip">httpd-2.4.69</a>
      <a href="/download/VS18/binaries/httpd-2.4.69-261002-Win32-VS18.zip">32 位应忽略</a>
      <a href="/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip.asc">asc 应忽略</a>
    "#;

    const VS16_PAGE: &str = r#"
      <a href="/download/VS16/binaries/httpd-2.4.57-win64-VS16.zip">httpd-2.4.57</a>
    "#;

    #[test]
    fn extracts_win64_zips_only() {
        let zs = parse_page(MAIN_PAGE);
        assert_eq!(zs.len(), 1, "32 位与 asc 链接不应计入");
        assert_eq!(zs[0].version, "2.4.69");
        assert_eq!(
            zs[0].url,
            "https://www.apachelounge.com/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip"
        );
    }

    #[test]
    fn merges_pages_and_marks_latest_recommended() {
        let list = merge_pages(&[MAIN_PAGE, VS16_PAGE], Prefer::Latest).unwrap();
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["2.4.69", "2.4.57"]
        );
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "2.4.69");
    }

    #[test]
    fn case_insensitive_win64_matches_vs16_naming() {
        let zs = parse_page(VS16_PAGE);
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].version, "2.4.57");
    }

    #[test]
    fn empty_page_yields_empty_list() {
        assert!(merge_pages(&["<html></html>"], Prefer::Latest).unwrap().is_empty());
    }
}
