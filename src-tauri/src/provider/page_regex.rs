//! 页面正则版本源：抓官网页面 → 正则提取版本 → 模板拼直链。
//! 适用于无版本 API 但页面（SSR/静态）含版本号的站点：
//! - Kiro：下载页直接含完整安装包 URL（url_template = "{match}"）
//! - ZCode：官网页面含 `releases/<版本>/windows-x64` 路径段（url_template 带 {version}）
//! 列表只暴露页面上的「最新」版本，resolve 时重新抓取，页面更新即跟随。

use super::{DownloadTarget, ProviderError, VersionInfo, VersionProvider};
use crate::manifest::{Item, ProviderDef};

pub struct PageRegexProvider;

fn cfg(item: &Item) -> Result<(String, String, String), ProviderError> {
    match item.provider.as_ref() {
        Some(ProviderDef::PageRegex { url, version_regex, url_template }) => {
            Ok((url.clone(), version_regex.clone(), url_template.clone()))
        }
        _ => Err(ProviderError::NoProvider),
    }
}

/// 纯解析：从页面文本提取所有版本号（捕获组 1），返回去重后的最高版本与对应整匹配。
/// 若正则命中的是完整 URL（含 https://），整匹配可直接作为下载地址。
pub fn pick_latest(html: &str, version_regex: &str) -> Result<(String, String), ProviderError> {
    let re = regex::Regex::new(version_regex).map_err(|e| ProviderError::Parse(format!("正则无效: {e}")))?;
    let mut best: Option<(String, String)> = None; // (版本, 整匹配)
    for cap in re.captures_iter(html) {
        let whole = cap.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
        let ver = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or(whole.clone());
        if ver.is_empty() {
            continue;
        }
        let better = match &best {
            Some((v, _)) => super::cmp_version(&ver, v) == std::cmp::Ordering::Greater,
            None => true,
        };
        if better {
            best = Some((ver, whole));
        }
    }
    best.ok_or_else(|| ProviderError::Index("页面中没有匹配到版本号".into()))
}

#[async_trait::async_trait]
impl VersionProvider for PageRegexProvider {
    async fn list_versions(&self, item: &Item) -> Result<Vec<VersionInfo>, ProviderError> {
        let (url, version_regex, _) = cfg(item)?;
        let body = crate::http::get_text(&url).await?;
        let (version, _) = pick_latest(&body, &version_regex)?;
        Ok(vec![VersionInfo { version, lts: false, lts_name: None, date: None, recommended: true }])
    }

    async fn resolve(&self, item: &Item, _version: &str) -> Result<Vec<DownloadTarget>, ProviderError> {
        let (url, version_regex, template) = cfg(item)?;
        let body = crate::http::get_text(&url).await?;
        let (version, whole) = pick_latest(&body, &version_regex)?;
        let real_url = template.replace("{version}", &version).replace("{match}", &whole);
        if !real_url.starts_with("https://") {
            return Err(ProviderError::Parse(format!("模板拼出的地址非法: {real_url}")));
        }
        let file_name = real_url.rsplit('/').next().unwrap_or("download.exe").to_string();
        Ok(vec![DownloadTarget { file_name, urls: vec![real_url], sha256: None, md5: None }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_highest_from_repeated_matches() {
        let html = r#"
          <a href="https://cdn.example.com/releases/3.15.1/windows-x64/ZCode-3.15.1-win-x64.exe">
          <a href="https://cdn.example.com/releases/3.14.4/windows-x64/ZCode-3.14.4-win-x64.exe">
        "#;
        let (ver, whole) = pick_latest(html, r"releases/(\d+\.\d+\.\d+)/windows-x64").unwrap();
        assert_eq!(ver, "3.15.1");
        assert!(whole.contains("3.15.1"), "整匹配应为最新版本的原文");
    }

    #[test]
    fn whole_match_captures_full_url() {
        let html = r#"<a href="https://prod.download.desktop.kiro.dev/releases/stable/win32-x64/signed/1.2.56/kiro-ide-1.2.56-stable-win32-x64.exe">"#;
        let (ver, whole) = pick_latest(
            html,
            r#"https://prod\.download\.desktop\.kiro\.dev/[^\s"'<>]*kiro-ide-(\d+\.\d+\.\d+)-stable-win32-x64\.exe"#,
        )
        .unwrap();
        assert_eq!(ver, "1.2.56");
        assert!(whole.starts_with("https://"));
        assert!(whole.ends_with("win32-x64.exe"));
    }

    #[test]
    fn no_match_is_error() {
        assert!(pick_latest("<html></html>", r"releases/(\d+\.\d+\.\d+)/windows-x64").is_err());
        assert!(pick_latest("ok", r"[invalid(").is_err());
    }

    #[tokio::test]
    async fn resolve_uses_manifest_items() {
        let m = crate::manifest::Manifest::load().unwrap();
        // Kiro：页面直链模式
        let t = super::PageRegexProvider
            .resolve(&m.get("kiro").unwrap().clone(), "最新")
            .await
            .unwrap_or_else(|e| panic!("kiro 解析失败: {e}"));
        assert!(t[0].file_name.starts_with("kiro-ide-"), "kiro 文件名异常: {}", t[0].file_name);
        assert!(t[0].urls[0].contains("prod.download.desktop.kiro.dev"));
        // ZCode：模板拼接模式
        let t2 = super::PageRegexProvider
            .resolve(&m.get("zcode").unwrap().clone(), "最新")
            .await
            .unwrap_or_else(|e| panic!("zcode 解析失败: {e}"));
        assert!(t2[0].file_name.starts_with("ZCode-"), "zcode 文件名异常: {}", t2[0].file_name);
        assert!(t2[0].urls[0].contains("cdn-zcode.z.ai"));
        let ver = &t2[0].file_name;
        let ver = &ver["ZCode-".len()..ver.len() - "-win-x64.exe".len()];
        assert!(t2[0].urls[0].contains(&format!("releases/{ver}/windows-x64")), "模板版本号应一致");
    }
}
