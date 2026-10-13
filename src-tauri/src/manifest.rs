//! 内嵌 TOML 清单的解析与校验。设计依据：设计文档第 8 节。

use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

pub const MANIFEST_TOML: &str = include_str!("../resources/manifest.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(rename = "item", default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub category: String,
    pub source: Source,
    pub kind: Kind,
    #[serde(default)] pub provider: Option<ProviderDef>,
    #[serde(default)] pub path: Option<String>,
    #[serde(default)] pub archive: Option<Archive>,
    #[serde(default)] pub requires: Vec<String>,
    #[serde(default)] pub actions: Vec<Action>,
    #[serde(default)] pub mirrors: Vec<Mirror>,
    #[serde(default)] pub detect: Option<Detect>,
    #[serde(default)] pub env: Option<EnvAction>,
    #[serde(default)] pub verify: Vec<Vec<String>>,
    #[serde(default)] pub note: Option<String>,
    #[serde(default)] pub expect_version: Option<String>,
}

impl Item {
    /// 免安装类需要用户指定安装目录。
    pub fn needs_install_dir(&self) -> bool {
        self.kind == Kind::Portable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source { Online, Local, Config }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind { Installer, Portable, Config }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Archive {
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prefer { Lts, Latest }

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderDef {
    NodeDist { prefer: Prefer },
    PythonFtp { prefer: Prefer },
    GithubRelease {
        repo: String,
        asset: String,
        #[serde(default)] mirror: Option<String>,
        prefer: Prefer,
    },
    MariadbRest { prefer: Prefer },
    StaticList { versions: Vec<StaticVersion> },
    Fixed { urls: Vec<String> },
    Rustup {
        url: String,
        #[serde(default)] mirror: Option<String>,
    },
    /// Go：`go.dev/dl/?mode=json&include=all`
    GoDl { prefer: Prefer },
    /// .NET SDK：`releases-index.json` 的受支持通道
    DotnetRelease { prefer: Prefer },
    /// Eclipse Temurin（OpenJDK）：`api.adoptium.net` 的特性版本
    Adoptium { prefer: Prefer },
    /// PHP：`windows.php.net` 目录列表（线程安全版 x64 zip）
    PhpWindows { prefer: Prefer },
    /// Miniconda：官方目录列表（清华镜像回落）
    Miniconda { prefer: Prefer },
    /// Bun：GitHub Releases 固定资产名，npmmirror 回落
    Bun { prefer: Prefer },
    /// nginx：`nginx.org` 下载页抓取（偶数次版本号 = 稳定线，视为 LTS）
    NginxOfficial { prefer: Prefer },
    /// Apache HTTP Server：Apache Lounge 下载页抓取（无国内镜像，单源）
    ApacheLounge { prefer: Prefer },
    /// MongoDB：`downloads.mongodb.org/current.json`（fastdl CDN，单源）
    MongodbFastdl { prefer: Prefer },
    /// 平铺版本 API：一次 GET 返回 {version, url, sha256hash}（腾讯 CodeBuddy/WorkBuddy 等）
    UpdateApi {
        url: String,
        #[serde(default)] version_field: Option<String>,
        #[serde(default)] url_field: Option<String>,
        #[serde(default)] sha_field: Option<String>,
    },
    /// Trae 版本 API：按 region（cn/sg）从 data.manifest.win32.versions[] 取 Windows x64
    TraeApi { region: String },
    /// 页面正则：抓官网页面，正则提取版本号，按模板拼出直链。
    /// url_template 支持 {version}（捕获组 1）与 {match}（整个匹配文本，用于正则直接命中完整 URL 时）。
    PageRegex { url: String, version_regex: String, url_template: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct StaticVersion {
    pub version: String,
    pub url: String,
    #[serde(default)] pub sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Mirror {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Detect {
    #[serde(default)] pub command: Option<Vec<String>>,
    #[serde(default)] pub registry_display_name_prefix: Option<String>,
    #[serde(default)] pub path: Option<String>,
    /// [文件路径, 需要包含的字符串]
    #[serde(default)] pub file_contains: Option<Vec<String>>,
    /// 环境变量名列表：全部存在且非空才视为已配置
    #[serde(default)] pub env: Option<Vec<String>>,
    #[serde(default)] pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EnvAction {
    #[serde(default)] pub set: BTreeMap<String, String>,
    #[serde(default)] pub path_append: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope { User, System }

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    EnvSet {
        scope: Scope,
        #[serde(default)] set: BTreeMap<String, String>,
        #[serde(default)] path_append: Vec<String>,
    },
    TomlMerge {
        file: String,
        #[serde(default)] set: Vec<TomlSetEntry>,
        #[serde(default, rename = "set_table")] set_table: Vec<TomlSetTableEntry>,
    },
    Command {
        program: String,
        args: Vec<String>,
        #[serde(default)] optional: bool,
    },
    SshKeygen {
        key_type: String,
        file: String,
        comment: String,
    },
    /// XML 合并：在根下 `container` 元素中按 `id_tag` 文本定位/追加 `child_tag` 块。
    /// 适用于浅层结构（Maven settings.xml、NuGet.Config），不支持深层嵌套。
    XmlMerge {
        file: String,
        root: String,
        container: String,
        child: String,
        id_tag: String,
        id_value: String,
        #[serde(default)]
        fields: BTreeMap<String, String>,
    },
    /// 整文件写入（覆盖）：仅用于本工具自托管的文件（如 Gradle init.d 下的固定命名脚本），写前备份。
    FileWrite {
        file: String,
        #[serde(default)]
        lines: Vec<String>,
    },
    /// 行追加：文件中（trim 后精确比较）不存在的行才追加到末尾，写前备份。用于 .Rprofile 等。
    FileEnsureLines {
        file: String,
        #[serde(default)]
        lines: Vec<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlSetEntry { pub path: String, pub value: String }

#[derive(Debug, Clone, Deserialize)]
pub struct TomlSetTableEntry { pub path: String, pub key: String, pub value: String }

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("清单解析失败: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("清单中存在重复 id: {0}")]
    DuplicateId(String),
    #[error("在线项缺少 provider: {0}")]
    MissingProvider(String),
    #[error("内置项缺少 path: {0}")]
    MissingPath(String),
    #[error("配置项缺少 actions: {0}")]
    MissingActions(String),
    #[error("免安装项缺少 archive: {0}")]
    MissingArchive(String),
    #[error("配置项 {0} 依赖了不存在的 id: {1}")]
    UnknownRequirement(String, String),
}

impl Manifest {
    pub fn load() -> Result<Self, ManifestError> {
        let m: Manifest = toml::from_str(MANIFEST_TOML)?;
        m.validate()?;
        Ok(m)
    }

    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    pub(crate) fn validate(&self) -> Result<(), ManifestError> {
        let mut seen = HashSet::new();
        for it in &self.items {
            if !seen.insert(it.id.clone()) {
                return Err(ManifestError::DuplicateId(it.id.clone()));
            }
        }
        let all_ids: HashSet<&str> = self.items.iter().map(|i| i.id.as_str()).collect();
        for it in &self.items {
            match (it.source, it.kind) {
                (Source::Online, Kind::Installer) | (Source::Online, Kind::Portable) => {
                    if it.provider.is_none() {
                        return Err(ManifestError::MissingProvider(it.id.clone()));
                    }
                }
                (Source::Local, _) => {
                    if it.path.is_none() {
                        return Err(ManifestError::MissingPath(it.id.clone()));
                    }
                }
                (Source::Config, _) if it.actions.is_empty() => {
                    return Err(ManifestError::MissingActions(it.id.clone()));
                }
                _ => {}
            }
            if it.kind == Kind::Portable && it.archive.is_none() {
                return Err(ManifestError::MissingArchive(it.id.clone()));
            }
            for r in &it.requires {
                if !all_ids.contains(r.as_str()) {
                    return Err(ManifestError::UnknownRequirement(it.id.clone(), r.clone()));
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn validate_for_test(&self) -> Result<(), ManifestError> {
        self.validate()
    }
}
