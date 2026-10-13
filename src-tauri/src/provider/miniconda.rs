//! Miniconda 版本源：官方 `repo.anaconda.com/miniconda/` 目录列表，
//! 拉不到或为空时回落清华 `mirrors.tuna.tsinghua.edu.cn/anaconda/miniconda/`。
//! 版本形如 `py314_26.7.1-1`（Python 版本 + conda 版本），公共 parse_version
//! 对该格式完全失效（首段非数字），因此这里自带数字段排序键。

use super::{DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const OFFICIAL: &str = "https://repo.anaconda.com/miniconda/";
const MIRROR: &str = "https://mirrors.tuna.tsinghua.edu.cn/anaconda/miniconda/";

pub struct MinicondaProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::Miniconda { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

/// 从版本串中抽出所有连续数字段做排序键：`py314_26.7.1-1` → [314, 26, 7, 1, 1]。
pub fn sort_key(version: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in version.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(cur.parse().unwrap_or(0));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur.parse().unwrap_or(0));
    }
    out
}

/// 纯解析：从目录列表 HTML 中提取 Miniconda3 Windows x64 安装包版本。
/// `prefer` 参数保留以对齐其他适配器签名；Miniconda 无 LTS，恒取最新。
pub fn parse_listing(html: &str, _prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let re = regex::Regex::new(r#"Miniconda3-(py\d+_[\d\.]+-\d+)-Windows-x86_64\.exe"#).expect("合法正则");
    let mut out: Vec<VersionInfo> = Vec::new();
    for cap in re.captures_iter(html) {
        out.push(VersionInfo {
            version: cap[1].to_string(),
            lts: false,
            lts_name: None,
            date: None,
            recommended: false,
        });
    }
    out.sort_by(|a, b| sort_key(&b.version).cmp(&sort_key(&a.version)));
    out.dedup_by(|a, b| a.version == b.version);
    // Miniconda 无 LTS 概念，recommended = 最新
    if let Some(first) = out.first_mut() {
        first.recommended = true;
    }
    Ok(out)
}

pub fn file_name(version: &str) -> String {
    format!("Miniconda3-{version}-Windows-x86_64.exe")
}

#[async_trait::async_trait]
impl VersionProvider for MinicondaProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let _prefer = prefer_of(item);
        // 官方优先；拉不到或解析为空时回落清华镜像
        let mut last_err: Option<String> = None;
        for src in [OFFICIAL, MIRROR] {
            match crate::http::get_text(src).await {
                Ok(body) => {
                    let list = parse_listing(&body, Prefer::Latest)?;
                    if !list.is_empty() {
                        return Ok(list);
                    }
                    last_err = Some(crate::tr!("列表为空", "List is empty"));
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        Err(ProviderError::Index(crate::tr!(
            "Miniconda 版本列表获取失败（官方与清华镜像）：{0}",
            "Failed to fetch the Miniconda version list (official and Tsinghua mirror): {0}",
            last_err.unwrap_or_default()
        )))
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let f = file_name(version);
        Ok(vec![DownloadTarget {
            file_name: f.clone(),
            urls: vec![format!("{OFFICIAL}{f}"), format!("{MIRROR}{f}")],
            sha256: None,
            md5: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"
      <a href="Miniconda3-latest-Windows-x86_64.exe">latest</a>
      <a href="Miniconda3-py314_26.7.1-1-Windows-x86_64.exe">a</a>
      <a href="Miniconda3-py313_25.1.1-2-Windows-x86_64.exe">b</a>
      <a href="Miniconda3-py310_22.11.1-1-Windows-x86_64.exe">c</a>
      <a href="Miniconda3-py39_4.12.0-Windows-x86_32.exe">32 位应忽略</a>
    "#;

    #[test]
    fn sort_key_extracts_digit_runs() {
        assert_eq!(sort_key("py314_26.7.1-1"), vec![314, 26, 7, 1, 1]);
        assert_eq!(sort_key("py39_4.12.0"), vec![39, 4, 12, 0]);
        assert!(sort_key("py314_26.7.1-1") > sort_key("py313_25.1.1-2"));
    }

    #[test]
    fn keeps_x64_only_and_sorts_by_python_then_conda() {
        let list = parse_listing(HTML, Prefer::Latest).unwrap();
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["py314_26.7.1-1", "py313_25.1.1-2", "py310_22.11.1-1"],
            "latest 占位文件与 32 位包不应出现"
        );
        assert!(list[0].recommended, "最新版应被推荐");
    }

    #[test]
    fn empty_listing_is_empty() {
        assert!(parse_listing("<html></html>", Prefer::Latest).unwrap().is_empty());
    }

    #[test]
    fn file_name_uses_version_verbatim() {
        assert_eq!(file_name("py314_26.7.1-1"), "Miniconda3-py314_26.7.1-1-Windows-x86_64.exe");
    }

    #[tokio::test]
    async fn resolve_builds_official_and_mirror_urls() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("miniconda").unwrap().clone();
        let t = MinicondaProvider.resolve(&item, "py314_26.7.1-1").await.unwrap();
        assert_eq!(t[0].urls.len(), 2);
        assert!(t[0].urls[0].starts_with("https://repo.anaconda.com/miniconda/"));
        assert!(t[0].urls[1].starts_with("https://mirrors.tuna.tsinghua.edu.cn/anaconda/miniconda/"));
    }
}
