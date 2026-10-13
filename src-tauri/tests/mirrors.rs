//! 只读集成测试：对清单里每个镜像地址真实请求一次，断言可用。
//! 容忍：405（方法不允许）、429（限流，服务在线）。
//! npm registry 根路径 404 属常态（npmmirror 例外），npm 项改用包路径探测。
//! 网络抖动：版本索引测试对每个地址最多重试 3 次。
//! 运行：cargo test --manifest-path src-tauri/Cargo.toml --test mirrors -- --nocapture

use devenv_assistant_lib::manifest::Manifest;

async fn status_of(url: &str) -> Result<u16, String> {
    let client = reqwest::Client::builder()
        .user_agent("devenv-assistant/1.0")
        // 大文件 CDN（如 fastdl.mongodb.org）冷连接握手可能超 20 秒，放宽避免误报
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;
    // 先 HEAD；404 或网络错误时补一次 GET
    // （部分网关对 HEAD 返回 404 但 GET 正常，如 Trae/CodeBuddy/WorkBuddy 的版本 API）
    match client.head(url).send().await {
        Ok(r) if r.status().as_u16() != 404 => Ok(r.status().as_u16()),
        Ok(_) => {
            let get = client.get(url).send().await.map_err(|e| format!("HEAD 404 且 GET 失败: {e}"))?;
            Ok(get.status().as_u16())
        }
        Err(e) => {
            let get = client.get(url).send().await.map_err(|e2| format!("{e}; GET 也失败: {e2}"))?;
            let _ = get;
            Err(format!("HEAD 失败: {e}"))
        }
    }
}

fn reachable_status(code: u16) -> bool {
    (200..300).contains(&code) || code == 405 || code == 429
}

/// npm / bun / pnpm / yarn registry 的根路径普遍 404，真实契约是「{base}<包名>」可达；
/// Maven / Gradle 仓库根路径同样 404，用真实 artifact 路径探测
fn probe_url(item_id: &str, url: &str) -> String {
    if matches!(item_id, "npm-mirror" | "bun-mirror" | "pnpm-mirror" | "yarn-mirror") {
        let base = if url.ends_with('/') { url.to_string() } else { format!("{url}/") };
        format!("{base}react")
    } else if matches!(item_id, "maven-mirror" | "gradle-mirror") {
        let base = if url.ends_with('/') { url.to_string() } else { format!("{url}/") };
        format!("{base}junit/junit/4.13.2/junit-4.13.2.pom")
    } else {
        url.to_string()
    }
}

#[tokio::test]
async fn all_config_mirrors_are_reachable() {
    let m = Manifest::load().expect("清单应能加载");
    let mut failures = Vec::new();
    for item in m.items.iter().filter(|i| !i.mirrors.is_empty()) {
        for mirror in &item.mirrors {
            let url = probe_url(&item.id, &mirror.url);
            match status_of(&url).await {
                Ok(code) if reachable_status(code) => {}
                Ok(code) => failures.push(format!("{} / {} -> {} (HTTP {})", item.id, mirror.name, mirror.url, code)),
                Err(e) => failures.push(format!("{} / {} -> {} ({})", item.id, mirror.name, mirror.url, e)),
            }
        }
    }
    assert!(failures.is_empty(), "以下镜像不可用:\n{}", failures.join("\n"));
}

#[tokio::test]
async fn version_indexes_are_reachable() {
    let urls = [
        "https://nodejs.org/dist/index.json",
        "https://www.python.org/ftp/python/",
        "https://api.github.com/repos/git-for-windows/git/releases?per_page=1",
        "https://downloads.mariadb.org/rest-api/mariadb/",
        // Go：版本列表（HEAD 返回 405 属正常，reachable_status 已容忍）
        "https://go.dev/dl/?mode=json&include=all",
        // .NET SDK：版本索引与某个通道的明细
        "https://dotnetcli.blob.core.windows.net/dotnet/release-metadata/releases-index.json",
        "https://builds.dotnet.microsoft.com/dotnet/release-metadata/10.0/releases.json",
        // Java：特性版本与某版本的最新 GA 构建
        "https://api.adoptium.net/v3/info/available_releases",
        "https://api.adoptium.net/v3/assets/feature_releases/21/ga?os=windows&architecture=x64&image_type=jdk&page_size=1",
        // Ruby / Bun：GitHub Releases（资产模板反解版本）
        "https://api.github.com/repos/oneclick/rubyinstaller2/releases?per_page=1",
        "https://api.github.com/repos/oven-sh/bun/releases?per_page=1",
        // PHP：官网 releases 目录列表与 302 跳转目标（唯一来源，无国内镜像）
        "https://windows.php.net/downloads/releases/",
        "https://downloads.php.net/~windows/releases/",
        // Miniconda：官方目录列表与清华回落列表
        "https://repo.anaconda.com/miniconda/",
        "https://mirrors.tuna.tsinghua.edu.cn/anaconda/miniconda/",
        // nginx：官网下载页（版本抓取源）
        "https://nginx.org/en/download.html",
        // Apache：Apache Lounge 下载页（当前版 + VS16 存档页）
        "https://www.apachelounge.com/download/",
        "https://www.apachelounge.com/download/VS16/",
        // MongoDB：current.json（459KB；勿加 full.json，50MB 会拖垮测试）
        "https://downloads.mongodb.org/current.json",
        // AI 工具的版本解析 API（清单直链的来源，API 挂了条目就失效）
        "https://api.trae.com.cn/icube/api/v1/native/version/trae/latest",
        "https://copilot.tencent.com/v2/update?platform=ide-win32-x64-user&version=1.0.0&x-machine-id=default",
        "https://www.codebuddy.ai/v2/update?platform=ide-win32-x64-user&version=1.0.0&x-machine-id=default",
        "https://www.workbuddy.cn/v2/update?platform=workbuddy-win32-x64-user&version=5.0.0&x-machine-id=default",
        "https://www.workbuddy.ai/v2/update?platform=workbuddy-win32-x64-user&version=5.0.0&x-machine-id=default",
        // Kiro / ZCode 的页面规则解析源（页面结构变了条目就失效）
        "https://kiro.dev/downloads/",
        "https://zcode.z.ai/en",
        // AI IDE 的长效 latest 直链（固定版本条目的 URL 已在入库前逐一实测）
        "https://aka.ms/vs/stable/vs_community.exe",
        "https://api2.cursor.sh/updates/download/golden/win32-x64-user/cursor/latest",
    ];
    let mut failures = Vec::new();
    for u in urls {
        let mut ok = false;
        let mut last = String::new();
        for _ in 0..3 {
            match status_of(u).await {
                Ok(code) if reachable_status(code) => { ok = true; break; }
                // GitHub API 未认证限流返回 403（配额 60 次/小时/IP），与 429 同属「服务在线」；
                // 应用运行时同样会遇到，换节点或稍后重试即可
                Ok(403) if u.starts_with("https://api.github.com/") => { ok = true; break; }
                Ok(code) => last = format!("HTTP {code}"),
                Err(e) => last = e,
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        if !ok { failures.push(format!("{u} ({last})")); }
    }
    assert!(failures.is_empty(), "版本索引不可达:\n{}", failures.join("\n"));
}

/// 新增运行环境的安装包回落地址也要真的能下到文件。
/// 用历史正式版做探针（各站都长期保留旧版本），避免目录根路径 404 造成的误判。
#[tokio::test]
async fn installer_mirrors_serve_real_files() {
    let urls = [
        // Go：官网 → Google CDN → 阿里云镜像
        "https://go.dev/dl/go1.21.0.windows-amd64.msi",
        "https://dl.google.com/go/go1.21.0.windows-amd64.msi",
        "https://mirrors.aliyun.com/golang/go1.21.0.windows-amd64.msi",
        // Java：清华 Adoptium 镜像目录
        "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/x64/windows/OpenJDK21U-jdk_x64_windows_hotspot_21.0.12.1_1.zip",
        // Miniconda：历史正式版做探针（官方 + 清华回落）
        "https://repo.anaconda.com/miniconda/Miniconda3-py310_23.3.1-0-Windows-x86_64.exe",
        "https://mirrors.tuna.tsinghua.edu.cn/anaconda/miniconda/Miniconda3-py310_23.3.1-0-Windows-x86_64.exe",
        // Bun：npmmirror 回落直链（历史正式版）
        "https://registry.npmmirror.com/-/binary/bun/bun-v1.1.0/bun-windows-x64.zip",
        // PHP：官网直链（历史正式版）
        "https://windows.php.net/downloads/releases/archives/php-8.2.0-Win32-vs16-x64.zip",
        // nginx：官网 + 华为云回落（历史稳定版）
        "https://nginx.org/download/nginx-1.28.3.zip",
        "https://mirrors.huaweicloud.com/nginx/nginx-1.28.3.zip",
        // Apache：Apache Lounge 页面列出的直链（VS16 存档；VS18 文件名带构建号会变，用稳定的历史版）
        "https://www.apachelounge.com/download/VS16/binaries/httpd-2.4.57-win64-VS16.zip",
        // MongoDB：fastdl CDN（历史版；国内镜像站均无 Windows 二进制）
        "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-7.0.14.zip",
        // PHP：归档页历史版本（8.0 以下；注意旧文件在 archives 子目录）
        "https://windows.php.net/downloads/releases/archives/php-7.3.33-Win32-VC15-x64.zip",
        // AI IDE：Qoder latest 固定路径（国内 / 国际）
        "https://download.qoder.com/release/latest/QoderIDEUserSetup-x64.exe",
        "https://qoder-ide-cn.oss-cn-hangzhou.aliyuncs.com/qoder/release/lastest/QoderCNIDEUserSetup-x64.exe",
    ];
    let mut failures = Vec::new();
    for u in urls {
        // 直链探测也容忍偶发网络抖动：最多重试 3 次
        let mut ok = false;
        let mut last = String::new();
        for _ in 0..3 {
            match status_of(u).await {
                Ok(code) if reachable_status(code) => { ok = true; break; }
                Ok(code) => last = format!("HTTP {code}"),
                Err(e) => last = e,
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        if !ok { failures.push(format!("{u} ({last})")); }
    }
    assert!(failures.is_empty(), "安装包镜像不可达:\n{}", failures.join("\n"));
}
