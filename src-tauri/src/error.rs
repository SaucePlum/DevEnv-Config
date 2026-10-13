/// 全局错误类型：所有 Tauri command 返回 `Result<T, AppError>`。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("清单错误: {0}")]
    Manifest(String),
    #[error("版本源错误: {0}")]
    Provider(String),
    #[error("下载失败: {0}")]
    Download(String),
    #[error("校验失败: {0}")]
    Checksum(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("注册表错误: {0}")]
    Registry(String),
    #[error("命令执行失败: {0}")]
    Command(String),
    #[error("安装器退出码 {0}")]
    InstallerExit(i32),
    #[error("解压失败: {0}")]
    Extract(String),
    #[error("配置失败: {0}")]
    Config(String),
    #[error("验证失败: {0}")]
    Verify(String),
    #[error("资源缺失: {0}")]
    MissingResource(String),
    #[error("{0}")]
    Other(String),
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
