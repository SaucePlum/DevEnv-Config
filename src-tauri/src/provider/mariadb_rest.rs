//! MariaDB 版本源：`https://downloads.mariadb.org/rest-api/mariadb/`。
//! 两步：大版本列表 → 该大版本下的小版本与其文件（含 sha256）。
//! 实测（2026-10）：小版本接口的 `releases` 是**以版本号为键的对象**而非数组；
//! 校验和嵌在 `files[].checksum.sha256sum`。
//! 大版本列表含 Preview/RC（如 13.2 Preview），仅保留 Stable / Long Term Support。
//! 镜像：清华 TUNA `https://mirrors.tuna.tsinghua.edu.cn/mariadb/`（仅回落安装包）。

use super::{sort_and_mark, DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, Prefer};

const API: &str = "https://downloads.mariadb.org/rest-api/mariadb/";

pub struct MariadbRestProvider;

fn prefer_of(item: &Item) -> Prefer {
    match item.provider.as_ref() {
        Some(crate::manifest::ProviderDef::MariadbRest { prefer }) => *prefer,
        _ => Prefer::Latest,
    }
}

#[derive(Debug, Clone)]
pub struct MajorRelease {
    pub release_id: String,
    /// Stable（滚动维护线）
    pub stable: bool,
    /// Long Term Support（长期支持线）
    pub lts: bool,
}

/// 从大版本列表 JSON 中取出 release_id，并标记 Stable / LTS（排除 Preview / RC）。
pub fn parse_majors(json: &str) -> Vec<MajorRelease> {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = v
        .get("major_releases")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    arr.into_iter()
        .filter_map(|m| {
            let id = m.get("release_id").and_then(|s| s.as_str())?;
            let status = m.get("release_status").and_then(|s| s.as_str()).unwrap_or("");
            let lts = status.contains("Long Term Support");
            let stable = lts || status.contains("Stable");
            Some(MajorRelease { release_id: id.to_string(), stable, lts })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct MariaFile {
    pub release_id: String,
    pub file_name: String,
    pub sha256: Option<String>,
}

/// 从某大版本下的 JSON 中取出所有 `*.msi`（winx64）文件及其版本号。
/// `releases` 兼容两种形态：以版本号为键的对象（线上实测）与数组。
pub fn parse_minors(json: &str) -> Vec<MariaFile> {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let releases = v.get("releases").cloned().unwrap_or(serde_json::Value::Null);
    let items: Vec<serde_json::Value> = match releases {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(m) => m.into_values().collect(),
        _ => vec![],
    };
    let mut out = Vec::new();
    for r in items {
        let release_id = match r.get("release_id").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let files = r.get("files").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        for f in files {
            let name = match f.get("file_name").and_then(|s| s.as_str()) {
                Some(s) => s,
                None => continue,
            };
            if !name.ends_with(".msi") || !name.contains("winx64") {
                continue;
            }
            let sha256 = f
                .get("checksum")
                .and_then(|c| c.get("sha256sum"))
                .and_then(|s| s.as_str())
                .map(String::from);
            out.push(MariaFile {
                release_id: release_id.clone(),
                file_name: name.to_string(),
                sha256,
            });
        }
    }
    out
}

/// 把大版本 + 小版本列表合成 VersionInfo 列表：
/// 只保留稳定版大版本，且列出其下**所有**已发布的小版本。
pub fn merge_versions(majors: &[MajorRelease], minors: &[MariaFile], prefer: Prefer) -> Vec<VersionInfo> {
    let mut out: Vec<VersionInfo> = majors
        .iter()
        .filter(|m| m.stable)
        .flat_map(|major| {
            let prefix = format!("{}.", major.release_id);
            let mut vs: Vec<VersionInfo> = minors
                .iter()
                .filter(|m| m.release_id.starts_with(&prefix))
                .map(|m| VersionInfo {
                    version: m.release_id.clone(),
                    lts: major.lts,
                    lts_name: None,
                    date: None,
                    recommended: false,
                })
                .collect();
            vs.sort_by(|a, b| super::cmp_version(&b.version, &a.version));
            vs
        })
        .collect();
    out.sort_by(|a, b| super::cmp_version(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    sort_and_mark(&mut out, prefer);
    out
}

#[async_trait::async_trait]
impl VersionProvider for MariadbRestProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let prefer = prefer_of(item);
        let majors_json = crate::http::get_text(API).await?;
        let majors = parse_majors(&majors_json);
        let stable: Vec<MajorRelease> = majors.into_iter().filter(|m| m.stable).collect();
        if stable.is_empty() {
            return Err(ProviderError::Index("MariaDB 稳定版大版本列表为空".into()));
        }
        let mut minors = Vec::new();
        // 查最新 5 个稳定大版本，覆盖主流选择；每个大版本下列出全部小版本
        let mut sorted = stable.clone();
        sorted.sort_by(|a, b| super::cmp_version(&b.release_id, &a.release_id));
        let mut last_err: Option<crate::http::HttpError> = None;
        for major in sorted.iter().take(5) {
            let url = format!("{API}{}/", major.release_id);
            match crate::http::get_text(&url).await {
                Ok(body) => minors.extend(parse_minors(&body)),
                Err(e) => last_err = Some(e),
            }
        }
        let list = merge_versions(&sorted, &minors, prefer);
        if list.is_empty() {
            let msg = match last_err {
                Some(e) => format!("MariaDB 未解析到可用版本: {e}"),
                None => "MariaDB 未解析到可用版本".to_string(),
            };
            return Err(ProviderError::Index(msg));
        }
        Ok(list)
    }

    async fn resolve(&self, _item: &Item, version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let major = version.rsplit_once('.').map(|(m, _)| m).unwrap_or(version);
        let url = format!("{API}{major}/");
        let body = crate::http::get_text(&url).await?;
        let files = parse_minors(&body);
        let f = files
            .into_iter()
            .find(|f| f.release_id == version)
            .ok_or_else(|| ProviderError::NoAsset(version.to_string()))?;
        let urls = installer_urls(version, &f.file_name);
        Ok(vec![DownloadTarget { file_name: f.file_name, urls, sha256: f.sha256, md5: None }])
    }
}

/// 安装包地址：官网 archive 优先，清华镜像随后。
pub fn installer_urls(version: &str, file_name: &str) -> Vec<String> {
    vec![
        format!("https://archive.mariadb.org/mariadb-{version}/winx64-packages/{file_name}"),
        format!("https://mirrors.tuna.tsinghua.edu.cn/mariadb/mariadb-{version}/winx64-packages/{file_name}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 线上实测的小版本接口形态：releases 是以版本号为键的对象，校验和嵌套在 checksum 里。
    const MINORS_JSON: &str = r#"{"releases": {"12.3.3": {"release_id": "12.3.3", "files": [
        {"file_name": "mariadb-12.3.3-winx64.zip", "package_type": "ZIP file"},
        {"file_name": "mariadb-12.3.3-winx64.msi", "package_type": "MSI package",
         "checksum": {"md5sum": "aa", "sha256sum": "abc"}},
        {"file_name": "mariadb-12.3.3-win32.msi"}
    ]}}}"#;

    /// 大版本接口实测形态：含 Preview / RC / Stable / LTS 状态。
    const MAJORS_JSON: &str = r#"{"major_releases": [
      {"release_id": "13.2", "release_status": "Preview"},
      {"release_id": "13.1", "release_status": "RC"},
      {"release_id": "13.0", "release_status": "Stable"},
      {"release_id": "12.3", "release_status": "Long Term Support"},
      {"release_id": "11.8", "release_status": "Stable"}
    ]}"#;

    #[test]
    fn parse_majors_filters_unstable_and_keeps_status() {
        let majors = parse_majors(MAJORS_JSON);
        let ids: Vec<&str> = majors.iter().map(|m| m.release_id.as_str()).collect();
        assert_eq!(ids, vec!["13.2", "13.1", "13.0", "12.3", "11.8"]);
        // Preview / RC 标记为非稳定；过滤发生在 merge / list_versions
        let stable_ids: Vec<&str> = majors.iter().filter(|m| m.stable).map(|m| m.release_id.as_str()).collect();
        assert_eq!(stable_ids, vec!["13.0", "12.3", "11.8"]);
        // 只有接口标注 Long Term Support 的线才是 LTS
        let lts_ids: Vec<&str> = majors.iter().filter(|m| m.lts).map(|m| m.release_id.as_str()).collect();
        assert_eq!(lts_ids, vec!["12.3"]);
    }

    #[test]
    fn parse_majors_tolerates_bad_json() {
        assert!(parse_majors("not json").is_empty());
    }

    #[test]
    fn parse_minors_reads_object_shaped_releases_with_nested_checksum() {
        let f = parse_minors(MINORS_JSON);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].file_name, "mariadb-12.3.3-winx64.msi");
        assert_eq!(f[0].sha256.as_deref(), Some("abc"));
    }

    #[test]
    fn parse_minors_also_supports_array_shape() {
        let json = r#"{"releases":[
          {"release_id":"11.4.9","files":[{"file_name":"mariadb-11.4.9-winx64.msi"}]}
        ]}"#;
        let f = parse_minors(json);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].release_id, "11.4.9");
    }

    #[test]
    fn merge_versions_lists_all_minors_of_stable_majors() {
        let majors = parse_majors(MAJORS_JSON);
        let minors = vec![
            MariaFile { release_id: "12.3.3".into(), file_name: "a3.msi".into(), sha256: None },
            MariaFile { release_id: "12.3.2".into(), file_name: "a2.msi".into(), sha256: None },
            MariaFile { release_id: "12.3.1".into(), file_name: "a1.msi".into(), sha256: None },
            MariaFile { release_id: "11.8.2".into(), file_name: "b.msi".into(), sha256: None },
        ];
        let list = merge_versions(&majors, &minors, Prefer::Latest);
        // 稳定线：13.0 / 12.3 / 11.8（12.3 的全部小版本列出，11.8.2 在列）
        assert_eq!(
            list.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["12.3.3", "12.3.2", "12.3.1", "11.8.2"]
        );
        // 「推荐」= 最新版本；LTS 标记来自大版本的支持类型
        assert_eq!(list.iter().find(|v| v.recommended).unwrap().version, "12.3.3");
        let v1233 = list.iter().find(|v| v.version == "12.3.3").unwrap();
        let v1182 = list.iter().find(|v| v.version == "11.8.2").unwrap();
        assert!(v1233.lts, "接口标注 12.3 为 Long Term Support");
        assert!(!v1182.lts, "接口标注 11.8 为 Stable");
    }

    #[test]
    fn installer_urls_put_official_first_then_mirror() {
        let urls = installer_urls("12.3.3", "mariadb-12.3.3-winx64.msi");
        assert_eq!(urls.len(), 2);
        assert_eq!(
            urls[0],
            "https://archive.mariadb.org/mariadb-12.3.3/winx64-packages/mariadb-12.3.3-winx64.msi"
        );
        assert!(urls[1].contains("tuna.tsinghua.edu.cn"));
    }

    #[test]
    fn major_is_derived_from_full_version() {
        assert_eq!("12.3.3".rsplit_once('.').map(|(m, _)| m).unwrap(), "12.3");
    }
}
