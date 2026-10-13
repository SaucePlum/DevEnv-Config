/// 全局错误类型：所有 Tauri command 返回 `Result<T, AppError>`。
/// Display 按当前界面语言输出，前端直接展示 `to_string()`。
#[derive(Debug)]
pub enum AppError {
    Manifest(String),
    Provider(String),
    Download(String),
    Checksum(String),
    Io(std::io::Error),
    Registry(String),
    Command(String),
    InstallerExit(i32),
    Extract(String),
    Config(String),
    Verify(String),
    MissingResource(String),
    Other(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::tr;
        match self {
            AppError::Manifest(s) => write!(f, "{}", tr!("清单错误: {0}", "Manifest error: {0}", s)),
            AppError::Provider(s) => write!(f, "{}", tr!("版本源错误: {0}", "Version source error: {0}", s)),
            AppError::Download(s) => write!(f, "{}", tr!("下载失败: {0}", "Download failed: {0}", s)),
            AppError::Checksum(s) => write!(f, "{}", tr!("校验失败: {0}", "Checksum failed: {0}", s)),
            AppError::Io(e) => write!(f, "{}", tr!("IO 错误: {0}", "IO error: {0}", e)),
            AppError::Registry(s) => write!(f, "{}", tr!("注册表错误: {0}", "Registry error: {0}", s)),
            AppError::Command(s) => write!(f, "{}", tr!("命令执行失败: {0}", "Command failed: {0}", s)),
            AppError::InstallerExit(c) => write!(f, "{}", tr!("安装器退出码 {0}", "Installer exit code {0}", c)),
            AppError::Extract(s) => write!(f, "{}", tr!("解压失败: {0}", "Extraction failed: {0}", s)),
            AppError::Config(s) => write!(f, "{}", tr!("配置失败: {0}", "Configuration failed: {0}", s)),
            AppError::Verify(s) => write!(f, "{}", tr!("验证失败: {0}", "Verification failed: {0}", s)),
            AppError::MissingResource(s) => write!(f, "{}", tr!("资源缺失: {0}", "Missing resource: {0}", s)),
            AppError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e)
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
