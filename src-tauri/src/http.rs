//! 共享 HTTP 客户端与文本请求封装。所有网络访问集中在此，便于统一 UA 与超时。

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("请求失败: {0}")]
    Request(#[from] reqwest::Error),
    #[error("HTTP {status}: {url}")]
    Status { status: u16, url: String },
    #[error("请求超时（60 秒）: {url}")]
    Timeout { url: String },
}

/// 此客户端不设总超时，供 download 模块流式复用；文本请求的超时在 `get_text` 内设置。
pub fn client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        let mut b = reqwest::Client::builder()
            .user_agent("devenv-assistant/1.0 (+windows)")
            .connect_timeout(Duration::from_secs(15));
        // 跟随 Windows 系统代理：VPN 工具多以系统代理模式工作，
        // 而 reqwest 默认只认环境变量，不读系统设置，会导致浏览器能访问 GitHub 而应用不能。
        // 无系统代理时不调用 .proxy()，保留 reqwest 对 HTTPS_PROXY 等环境变量的默认支持。
        if let Some(p) = system_proxy_url() {
            if let Ok(px) = reqwest::Proxy::all(&p) {
                b = b.proxy(px);
            }
        }
        b.build().expect("构建 HTTP 客户端失败")
    })
}

/// 解析 WinINET ProxyServer 字符串为代理 URL。支持三种形态：
/// "127.0.0.1:7890"（http/https 共用）、"http=…;https=…;socks=…"（分协议）、自带协议头的写法。
fn parse_proxy_server(server: &str) -> Option<String> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }
    let (addr, is_socks) = if server.contains('=') {
        let mut http: Option<String> = None;
        let mut https: Option<String> = None;
        let mut socks: Option<String> = None;
        for part in server.split(';') {
            let Some((k, v)) = part.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            if v.is_empty() {
                continue;
            }
            match k {
                "http" => http = Some(v.to_string()),
                "https" => https = Some(v.to_string()),
                "socks" => socks = Some(v.to_string()),
                _ => {}
            }
        }
        // 选择顺序：https > http > socks（应用主要发 https 请求）
        if let Some(a) = https {
            (a, false)
        } else if let Some(a) = http {
            (a, false)
        } else if let Some(a) = socks {
            (a, true)
        } else {
            return None;
        }
    } else {
        (server.to_string(), false)
    };
    if addr.contains("://") {
        Some(addr)
    } else if is_socks {
        Some(format!("socks5://{addr}"))
    } else {
        Some(format!("http://{addr}"))
    }
}

/// 读取 Windows 系统代理（WinINET 设置）。未启用时返回 None。
fn system_proxy_url() -> Option<String> {
    use winreg::enums::*;
    let settings = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    if enabled != 1 {
        return None;
    }
    let server: String = settings.get_value("ProxyServer").ok()?;
    parse_proxy_server(&server)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proxy_server_single_addr() {
        assert_eq!(parse_proxy_server("127.0.0.1:7890").as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[test]
    fn parse_proxy_server_per_protocol_prefers_https() {
        assert_eq!(
            parse_proxy_server("ftp=1.1.1.1:21;http=2.2.2.2:80;https=3.3.3.3:443").as_deref(),
            Some("http://3.3.3.3:443")
        );
    }

    #[test]
    fn parse_proxy_server_socks_entry_gets_socks5_scheme() {
        assert_eq!(
            parse_proxy_server("socks=127.0.0.1:7891;http=127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(parse_proxy_server("socks=127.0.0.1:7891").as_deref(), Some("socks5://127.0.0.1:7891"));
    }

    #[test]
    fn parse_proxy_server_keeps_explicit_scheme() {
        assert_eq!(
            parse_proxy_server("socks5://127.0.0.1:7891").as_deref(),
            Some("socks5://127.0.0.1:7891")
        );
    }

    #[test]
    fn parse_proxy_server_rejects_empty() {
        assert_eq!(parse_proxy_server("  "), None);
    }
}

/// 单次文本请求的总超时：连接超时只管握手，对端接受连接后停滞时仍会无限挂起。
const TEXT_TIMEOUT: Duration = Duration::from_secs(60);

/// GET 文本，非 2xx 视为失败。自动重试 1 次（应对瞬时网络抖动）。
pub async fn get_text(url: &str) -> Result<String, HttpError> {
    let mut last: Option<HttpError> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        match get_text_once(url).await {
            Ok(s) => return Ok(s),
            Err(e) => last = Some(e),
        }
    }
    Err(last.expect("至少执行过一次请求"))
}

async fn get_text_once(url: &str) -> Result<String, HttpError> {
    let fut = client().get(url).send();
    let resp = match tokio::time::timeout(TEXT_TIMEOUT, fut).await {
        Ok(r) => r?,
        Err(_) => return Err(HttpError::Timeout { url: url.to_string() }),
    };
    let status = resp.status();
    if !status.is_success() {
        return Err(HttpError::Status { status: status.as_u16(), url: url.to_string() });
    }
    // 响应体读取同样限长时：对端缓慢滴流时不再无限等待
    match tokio::time::timeout(TEXT_TIMEOUT, resp.text()).await {
        Ok(t) => t.map_err(HttpError::from),
        Err(_) => Err(HttpError::Timeout { url: url.to_string() }),
    }
}
