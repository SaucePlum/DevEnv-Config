//! 版本源适配器。设计依据：设计文档第 5 节。

use crate::manifest::{Item, Prefer, ProviderDef};
use serde::{Deserialize, Serialize};

pub mod adoptium;
pub mod apache_lounge;
pub mod bun;
pub mod dotnet_release;
pub mod fixed;
pub mod github_release;
pub mod go_dl;
pub mod mariadb_rest;
pub mod miniconda;
pub mod mongodb_fastdl;
pub mod nginx_official;
pub mod node_dist;
pub mod page_regex;
pub mod php_windows;
pub mod python_ftp;
pub mod rustup;
pub mod static_list;
pub mod trae_api;
pub mod update_api;

#[derive(Debug)]
pub enum ProviderError {
    NoProvider,
    Index(String),
    Parse(String),
    NoAsset(String),
    Http(crate::http::HttpError),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::tr;
        match self {
            ProviderError::NoProvider => f.write_str(&tr!("该项无版本源定义", "No version source defined for this item")),
            ProviderError::Index(s) => write!(f, "{}", tr!("无法获取版本列表: {0}", "Failed to fetch version list: {0}", s)),
            ProviderError::Parse(s) => write!(f, "{}", tr!("无法解析版本列表: {0}", "Failed to parse version list: {0}", s)),
            ProviderError::NoAsset(s) => write!(f, "{}", tr!("没有匹配的下载文件: {0}", "No matching download file: {0}", s)),
            ProviderError::Http(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<crate::http::HttpError> for ProviderError {
    fn from(e: crate::http::HttpError) -> Self {
        ProviderError::Http(e)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub lts: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lts_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// 是否为本项默认选中（最新 LTS / 最新稳定）
    pub recommended: bool,
}

#[derive(Debug, Clone)]
pub struct DownloadTarget {
    /// 保存到缓存时使用的文件名
    pub file_name: String,
    /// 依次尝试的下载地址：官网优先，镜像随后
    pub urls: Vec<String>,
    pub sha256: Option<String>,
    pub md5: Option<String>,
}

#[async_trait::async_trait]
pub trait VersionProvider: Send + Sync {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError>;
    async fn resolve(&self, item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError>;
}

/// 把版本号解析成可比较的数字序列（忽略非数字前后缀）。
pub fn parse_version(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split('.')
        .take_while(|s| s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
        .filter_map(|s| {
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().ok()
        })
        .collect()
}

pub fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    parse_version(a).cmp(&parse_version(b))
}

/// 按版本号降序排列，并把默认项标记为 recommended。
pub fn sort_and_mark(list: &mut [VersionInfo], prefer: Prefer) {
    list.sort_by(|a, b| cmp_version(&b.version, &a.version));
    for v in list.iter_mut() {
        v.recommended = false;
    }
    let idx = match prefer {
        Prefer::Latest => list
            .iter()
            .enumerate()
            .max_by(|a, b| cmp_version(&a.1.version, &b.1.version))
            .map(|(i, _)| i),
        Prefer::Lts => list
            .iter()
            .enumerate()
            .filter(|(_, v)| v.lts)
            .max_by(|a, b| cmp_version(&a.1.version, &b.1.version))
            .map(|(i, _)| i)
            .or_else(|| {
                list.iter()
                    .enumerate()
                    .max_by(|a, b| cmp_version(&a.1.version, &b.1.version))
                    .map(|(i, _)| i)
            }),
    };
    if let Some(i) = idx {
        list[i].recommended = true;
    }
}

/// 替换地址模板里的 `{version}` 与 `{tag}`。
pub fn fill_url_template(tpl: &str, version: &str, tag: &str) -> String {
    tpl.replace("{version}", version).replace("{tag}", tag)
}

pub fn provider_for(def: &ProviderDef) -> Box<dyn VersionProvider> {
    match def {
        ProviderDef::NodeDist { .. } => Box::new(node_dist::NodeDistProvider),
        ProviderDef::PythonFtp { .. } => Box::new(python_ftp::PythonFtpProvider),
        ProviderDef::GithubRelease { .. } => Box::new(github_release::GithubReleaseProvider),
        ProviderDef::MariadbRest { .. } => Box::new(mariadb_rest::MariadbRestProvider),
        ProviderDef::StaticList { .. } => Box::new(static_list::StaticListProvider),
        ProviderDef::Fixed { .. } => Box::new(fixed::FixedProvider),
        ProviderDef::Rustup { .. } => Box::new(rustup::RustupProvider),
        ProviderDef::GoDl { .. } => Box::new(go_dl::GoDlProvider),
        ProviderDef::DotnetRelease { .. } => Box::new(dotnet_release::DotnetReleaseProvider),
        ProviderDef::Adoptium { .. } => Box::new(adoptium::AdoptiumProvider),
        ProviderDef::PhpWindows { .. } => Box::new(php_windows::PhpWindowsProvider),
        ProviderDef::Miniconda { .. } => Box::new(miniconda::MinicondaProvider),
        ProviderDef::Bun { .. } => Box::new(bun::BunProvider),
        ProviderDef::NginxOfficial { .. } => Box::new(nginx_official::NginxOfficialProvider),
        ProviderDef::ApacheLounge { .. } => Box::new(apache_lounge::ApacheLoungeProvider),
        ProviderDef::MongodbFastdl { .. } => Box::new(mongodb_fastdl::MongodbFastdlProvider),
        ProviderDef::UpdateApi { .. } => Box::new(update_api::UpdateApiProvider),
        ProviderDef::TraeApi { .. } => Box::new(trae_api::TraeApiProvider),
        ProviderDef::PageRegex { .. } => Box::new(page_regex::PageRegexProvider),
    }
}

pub async fn list_versions(item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
    let def = item.provider.as_ref().ok_or(ProviderError::NoProvider)?;
    provider_for(def).list_versions(item).await
}

pub async fn resolve(item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
    let def = item.provider.as_ref().ok_or(ProviderError::NoProvider)?;
    provider_for(def).resolve(item, version).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(ver: &str, lts: bool) -> VersionInfo {
        VersionInfo { version: ver.into(), lts, lts_name: None, date: None, recommended: false }
    }

    #[test]
    fn parse_version_handles_prefix_and_suffix() {
        assert_eq!(parse_version("v1.2.3"), vec![1, 2, 3]);
        assert_eq!(parse_version("1.2.3.4"), vec![1, 2, 3, 4]);
        assert_eq!(parse_version("2.47.1.2"), vec![2, 47, 1, 2]);
        assert_eq!(parse_version("3.14.0rc1"), vec![3, 14, 0]);
    }

    #[test]
    fn cmp_version_orders_numerically() {
        assert!(cmp_version("1.10.0", "1.9.0") == std::cmp::Ordering::Greater);
        assert!(cmp_version("2.0.0", "2.0") > std::cmp::Ordering::Equal);
    }

    #[test]
    fn lts_prefer_picks_highest_lts() {
        let mut list = vec![v("24.21.0", true), v("25.0.0", false), v("22.20.0", true)];
        sort_and_mark(&mut list, Prefer::Lts);
        assert_eq!(list[0].version, "25.0.0");
        let pick = list.iter().find(|x| x.recommended).unwrap();
        assert_eq!(pick.version, "24.21.0");
    }

    #[test]
    fn lts_prefer_falls_back_to_latest_when_no_lts() {
        let mut list = vec![v("1.2.0", false), v("1.3.0", false)];
        sort_and_mark(&mut list, Prefer::Latest);
        assert_eq!(list.iter().find(|x| x.recommended).unwrap().version, "1.3.0");
    }

    #[test]
    fn latest_prefer_picks_highest_overall() {
        let mut list = vec![v("24.21.0", true), v("25.0.0", false)];
        sort_and_mark(&mut list, Prefer::Latest);
        assert_eq!(list.iter().find(|x| x.recommended).unwrap().version, "25.0.0");
    }

    #[test]
    fn sort_and_mark_on_empty_list_is_noop() {
        let mut list: Vec<VersionInfo> = Vec::new();
        sort_and_mark(&mut list, Prefer::Lts);
        assert!(list.is_empty());
    }

    #[test]
    fn fill_template_replaces_both_placeholders() {
        assert_eq!(
            fill_url_template("a/{tag}/b/{version}/c", "1.2.3", "v1.2.3"),
            "a/v1.2.3/b/1.2.3/c"
        );
    }
}
