//! PHP 版本源：`windows.php.net/downloads/releases/`（当前版本）+ `/archives/`（历史版本）。
//! 只收录线程安全版（TS）x64 zip；NTS 文件名带 `-nts-` 段，天然不匹配。
//! 主页保留各版本线最新包（8.0.30 …），更早版本（5.5 → 7.4）在 archives 页，
//! 两页合并后覆盖 5.5.38 起的全部 TS x64 包。命名区分 vs16 / VC15 / VC14 / VC11（大小写混用）。
//! 该地址 302 跳转到 `downloads.php.net/~windows/releases/`，故失败时直接请求跳转目标；
//! 国内无镜像，风险已接受（与 Redis 走 GitHub 同理）。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const BASE: &str = "https://windows.php.net/downloads/";
const FALLBACK: &str = "https://downloads.php.net/~windows/";

/// (页面路径, 页面标识)。归档页仅作版本列表补充与旧版下载，主页优先。
const PAGES: [(&str, &str); 2] = [("releases/", "releases/"), ("releases/archives/", "releases/archives/")];

pub struct PhpWindowsProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::PhpWindows { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

/// (版本号, 文件名, 所在页面)。
#[derive(Debug, Clone)]
pub struct PhpZip {
    pub version: String,
    pub file_name: String,
    pub page: &'static str,
}

/// 单页解析：线程安全版 x64 zip（vs* / VC* 大小写混用，统一大小写不敏感）。
/// 正则要求版本号后紧跟 `-Win32-`，NTS（`-nts-Win32-`）不会命中。
pub fn parse_page(html: &str, page: &'static str) -> Vec<PhpZip> {
    let re = regex::Regex::new(r#"(?i)php-(\d+\.\d+\.\d+)-Win32-(?:vs\d+|VC\d+)-x64\.zip"#).expect("合法正则");
    re.captures_iter(html)
        .map(|cap| PhpZip { version: cap[1].to_string(), file_name: cap[0].to_string(), page })
        .collect()
}

/// 合并多页：同版本保留优先出现的条目（主页优先于归档页）。
pub fn merge_pages(pages: &[(String, &'static str)], prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let mut zips: Vec<PhpZip> = Vec::new();
    for (html, page) in pages {
        for z in parse_page(html, page) {
            if !zips.iter().any(|x| x.version == z.version) {
                zips.push(z);
            }
        }
    }
    let mut out: Vec<VersionInfo> = zips
        .into_iter()
        .map(|z| VersionInfo { version: z.version, lts: false, lts_name: None, date: None, recommended: false })
        .collect();
    out.sort_by(|a, b| super::cmp_version(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

/// 依次拉取列表：官网 → 302 跳转目标（官网抖动时绕过跳转直达）。
async fn fetch_page(path: &str) -> Result<String, ProviderError> {
    let mut last: Option<ProviderError> = None;
    for base in [BASE, FALLBACK] {
        match crate::http::get_text(&format!("{base}{path}")).await {
            Ok(body) => return Ok(body),
            Err(e) => last = Some(e.into()),
        }
    }
    Err(last.unwrap_or(ProviderError::Index(crate::tr!("PHP 列表地址为空", "PHP list URL is empty"))))
}

#[async_trait::async_trait]
impl VersionProvider for PhpWindowsProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let mut pages: Vec<(String, &'static str)> = Vec::new();
        for (i, (path, page)) in PAGES.iter().enumerate() {
            match fetch_page(path).await {
                Ok(body) => pages.push((body, page)),
                // 归档页拉取失败不阻塞（主页版本仍在），主页失败才整体报错
                Err(e) if i > 0 && !pages.is_empty() => {
                    let _ = e;
                }
                Err(e) => return Err(e),
            }
        }
        let list = merge_pages(&pages, prefer)?;
        // 列表页结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index(crate::tr!("没有解析到可用的 PHP 版本", "No usable PHP version parsed")));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        for (path, page) in PAGES {
            let body = fetch_page(path).await?;
            if let Some(z) = parse_page(&body, page).into_iter().find(|z| z.version == version) {
                // 下载地址跟随文件所在页面：主页文件在 releases/，归档页文件在 releases/archives/
                return Ok(vec![DownloadTarget {
                    file_name: z.file_name.clone(),
                    urls: vec![format!("{BASE}{}{}", z.page, z.file_name)],
                    sha256: None,
                    md5: None,
                }]);
            }
        }
        Err(ProviderError::NoAsset(version.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN_HTML: &str = r#"
      <a href="php-8.5.11-nts-Win32-vs17-x64.zip">nts</a>
      <a href="php-8.5.11-Win32-vs17-x64.zip">ts 8.5</a>
      <a href="php-8.0.30-Win32-vs16-x64.zip">ts 8.0.30（主页保留的旧版本线）</a>
      <a href="extras/">extras</a>
    "#;

    const ARCHIVE_HTML: &str = r#"
      <a href="php-7.4.33-Win32-vs16-x64.zip">7.4</a>
      <a href="php-7.3.33-nts-Win32-VC15-x64.zip">nts 7.3 不应匹配</a>
      <a href="php-7.3.33-Win32-VC15-x64.zip">7.3</a>
      <a href="php-5.6.40-Win32-VC11-x64.zip">5.6</a>
      <a href="php-7.4.33-Win32-vs16-x64.zip">重复</a>
    "#;

    #[test]
    fn keeps_ts_only_with_mixed_case_naming() {
        // parse_page 不去重（去重发生在 merge_pages），只验证 TS 匹配与命名
        let zs = parse_page(ARCHIVE_HTML, "releases/archives/");
        assert_eq!(zs.len(), 4, "NTS 不应匹配：{}", zs.iter().map(|z| z.version.as_str()).collect::<Vec<_>>().join(","));
        assert!(zs.iter().any(|z| z.file_name == "php-7.3.33-Win32-VC15-x64.zip"));
        assert!(!zs.iter().any(|z| z.version == "5.6.40" && z.file_name.contains("nts")));
        assert!(zs.iter().all(|z| !z.file_name.contains("nts")));
    }

    #[test]
    fn merges_pages_main_first() {
        let pages = vec![
            (MAIN_HTML.to_string(), "releases/"),
            (ARCHIVE_HTML.to_string(), "releases/archives/"),
        ];
        let list = merge_pages(&pages, Prefer::Latest).unwrap();
        let versions: Vec<&str> = list.iter().map(|v| v.version.as_str()).collect();
        assert_eq!(
            versions,
            vec!["8.5.11", "8.0.30", "7.4.33", "7.3.33", "5.6.40"],
            "主页与归档页合并、数字降序"
        );
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "8.5.11");
    }

    #[test]
    fn empty_page_yields_empty_list() {
        assert!(merge_pages(&[("<html></html>".to_string(), "releases/")], Prefer::Latest)
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn resolve_uses_manifest_item() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("php").unwrap().clone();
        // 归档页历史版本：验证旧版解析且下载地址指向 archives 目录（需要能访问 windows.php.net）
        let t = PhpWindowsProvider.resolve(&item, "7.3.33").await.unwrap_or_else(|e| {
            panic!("解析失败: {e}（需要能访问 windows.php.net）")
        });
        assert_eq!(t[0].file_name, "php-7.3.33-Win32-VC15-x64.zip");
        assert!(
            t[0].urls[0].contains("releases/archives/"),
            "归档页文件应指向 archives 目录: {}",
            t[0].urls[0]
        );
        // 主页版本：下载地址在 releases/ 下
        let t2 = PhpWindowsProvider.resolve(&item, "8.4.26").await.unwrap();
        assert!(t2[0].urls[0].contains("releases/php-"), "主页文件应在 releases/ 下: {}", t2[0].urls[0]);
        assert!(!t2[0].urls[0].contains("archives"));
    }
}
