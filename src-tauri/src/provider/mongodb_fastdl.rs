//! MongoDB 版本源：`https://downloads.mongodb.org/current.json`（459KB，勿用 50MB 的 full.json）。
//! 该 JSON 每条版本线列一个当前生产版，全部标 production/lts，推荐位取最高版本。
//! Windows 社区包在 `downloads[]` 中：target=windows、arch=x86_64、edition=base，
//! `archive` 内含直链与 sha256（校验和免费拿到）。
//! fastdl CDN 国内可直连（实测 HEAD 200）；国内镜像站均无 Windows 二进制（实测 404），单源。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer, ProviderDef};

const CURRENT: &str = "https://downloads.mongodb.org/current.json";

pub struct MongodbFastdlProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(ProviderDef::MongodbFastdl { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

#[derive(serde::Deserialize)]
struct RawCurrent {
    #[serde(default)]
    versions: Vec<RawVersion>,
}

#[derive(serde::Deserialize)]
struct RawVersion {
    version: String,
    #[serde(default)]
    lts_release: bool,
    #[serde(default)]
    development_release: bool,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    downloads: Vec<RawDownload>,
}

#[derive(serde::Deserialize)]
struct RawDownload {
    #[serde(default)]
    target: String,
    #[serde(default)]
    arch: String,
    #[serde(default)]
    edition: String,
    #[serde(default)]
    archive: Option<RawArchive>,
}

#[derive(serde::Deserialize)]
struct RawArchive {
    url: String,
    #[serde(default)]
    sha256: Option<String>,
}

/// Windows x86_64 社区包（edition=base）。
#[derive(Debug, Clone)]
pub struct MongoZip {
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
}

/// 纯解析：取每条版本线的 Windows 社区 zip。
pub fn parse_current(json: &str, prefer: Prefer) -> Result<Vec<VersionInfo>, ProviderError> {
    let cur: RawCurrent = serde_json::from_str(json).map_err(|e| ProviderError::Parse(e.to_string()))?;
    let mut out: Vec<VersionInfo> = cur
        .versions
        .iter()
        .filter(|v| !v.development_release)
        .map(|v| VersionInfo {
            version: v.version.clone(),
            // 接口对列出的各线当前生产版均标 lts_release=true，尊重接口语义
            lts: v.lts_release,
            lts_name: None,
            date: v.date.clone(),
            recommended: false,
        })
        .collect();
    sort_and_mark(&mut out, prefer);
    Ok(out)
}

/// 从 JSON 中找出某版本的 Windows 社区 zip 直链。
pub fn find_zip(json: &str, version: &str) -> Option<MongoZip> {
    let cur: RawCurrent = serde_json::from_str(json).ok()?;
    for v in cur.versions {
        if v.version != version {
            continue;
        }
        for d in &v.downloads {
            if d.target == "windows" && d.arch == "x86_64" && d.edition == "base" {
                if let Some(a) = &d.archive {
                    return Some(MongoZip {
                        version: v.version.clone(),
                        url: a.url.clone(),
                        sha256: a.sha256.clone(),
                    });
                }
            }
        }
    }
    None
}

#[async_trait::async_trait]
impl VersionProvider for MongodbFastdlProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let body = crate::http::get_text(CURRENT).await?;
        let list = parse_current(&body, prefer)?;
        // 索引结构变化时宁可报错，也不要静默给出空列表
        if list.is_empty() {
            return Err(ProviderError::Index("没有解析到可用的 MongoDB 版本".into()));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let body = crate::http::get_text(CURRENT).await?;
        let z = find_zip(&body, version).ok_or_else(|| ProviderError::NoAsset(version.to_string()))?;
        let file_name = z.url.rsplit('/').next().unwrap_or("mongodb.zip").to_string();
        Ok(vec![DownloadTarget { file_name, urls: vec![z.url], sha256: z.sha256, md5: None }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 线上实测形态（截取）：enterprise 与 base 条目并存，archive 带 sha256。
    const JSON: &str = r#"{"versions":[
      {"version":"9.0.2","production_release":true,"lts_release":true,"development_release":false,"date":"09/03/2026",
       "downloads":[
         {"target":"windows","arch":"x86_64","edition":"enterprise","archive":{"url":"https://downloads.mongodb.com/windows/mongodb-windows-x86_64-enterprise-9.0.2.zip"}},
         {"target":"windows","arch":"x86_64","edition":"base","archive":{"url":"https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-9.0.2.zip","sha256":"aaa"}}
       ]},
      {"version":"8.0.32","production_release":true,"lts_release":true,"development_release":false,
       "downloads":[
         {"target":"windows","arch":"x86_64","edition":"base","archive":{"url":"https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-8.0.32.zip","sha256":"bbb"}},
         {"target":"rhel9","arch":"x86_64","edition":"base","archive":{"url":"https://x/rhel.zip"}}
       ]},
      {"version":"99.0.0","development_release":true,"downloads":[]}
    ]}"#;

    #[test]
    fn lists_production_lines_only() {
        let list = parse_current(JSON, Prefer::Latest).unwrap();
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["9.0.2", "8.0.32"],
            "开发线不应出现"
        );
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "9.0.2");
        assert!(list.iter().all(|v| v.lts), "接口对各线当前生产版均标 lts");
    }

    #[test]
    fn find_zip_picks_windows_base_edition() {
        let z = find_zip(JSON, "9.0.2").unwrap();
        assert!(z.url.contains("fastdl.mongodb.org"));
        assert!(!z.url.contains("enterprise"), "应选社区版而非企业版");
        assert_eq!(z.sha256.as_deref(), Some("aaa"));
        // 非 Windows 条目与无 base 条目的版本都取不到
        assert!(find_zip(JSON, "99.0.0").is_none());
        assert!(find_zip(JSON, "1.0.0").is_none());
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(parse_current("not json", Prefer::Latest).is_err());
        assert!(parse_current("{}", Prefer::Latest).unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolve_uses_manifest_item() {
        let m = crate::manifest::Manifest::load().unwrap();
        let item = m.get("mongodb").unwrap().clone();
        // 走真实 API（current.json 是稳定端点）；瞬时网络抖动（连接被重置等）跳过而非误报，
        // 解析 / 无资产类错误仍然失败
        let list = match MongodbFastdlProvider.list_versions(&item).await {
            Ok(l) => l,
            Err(e) if e.to_string().contains("请求失败") => {
                eprintln!("网络抖动，跳过该用例: {e}");
                return;
            }
            Err(e) => panic!("{e}"),
        };
        assert!(!list.is_empty());
        let rec = list.iter().find(|v| v.recommended).unwrap();
        let t = MongodbFastdlProvider.resolve(&item, &rec.version).await.unwrap();
        assert!(t[0].file_name.starts_with("mongodb-windows-x86_64-"));
        assert!(t[0].urls[0].contains("fastdl.mongodb.org"));
        assert!(t[0].sha256.is_some(), "fastdl 包应自带 sha256 校验和");
    }
}
