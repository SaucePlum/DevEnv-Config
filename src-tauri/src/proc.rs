//! 子进程执行与查找。检测、验证、tar、robocopy 都走这里，统一无窗口与超时。

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    pub fn combined(&self) -> String {
        let mut s = self.stdout.trim().to_string();
        let e = self.stderr.trim();
        if !e.is_empty() {
            if !s.is_empty() {
                s.push('\n');
            }
            s.push_str(e);
        }
        s
    }
    /// 从输出中抓第一个形如 1.2.3 的版本号。优先匹配含 "version" 的行（大小写不敏感），
    /// 全无命中再退回全文第一处匹配。注意：输出中的日期等数字序列可能干扰，
    /// 调用点应尽量用 `--version` 类命令获取输出。
    pub fn first_version(&self) -> Option<String> {
        let re = regex::Regex::new(r"\d+\.\d+(\.\d+)*").ok()?;
        let combined = self.combined();
        combined
            .lines()
            .find(|l| l.to_lowercase().contains("version"))
            .and_then(|l| re.find(l))
            .or_else(|| re.find(&combined))
            .map(|m| m.as_str().to_string())
    }
}

fn exe_names(program: &str) -> Vec<String> {
    if Path::new(program).extension().is_some() {
        return vec![program.to_string()];
    }
    let mut v = Vec::new();
    // Windows 下 PATHEXT 候选优先：裸名可能是给 Unix shell 用的脚本
    // （如 nodejs 目录下无扩展名的 npm），无法作为 Win32 程序启动（os error 193）。
    if cfg!(windows) {
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        for e in exts.split(';').filter(|s| !s.is_empty()) {
            v.push(format!("{program}{e}"));
        }
    }
    v.push(program.to_string());
    v
}

/// 在 PATH 中查找可执行文件。
pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.is_absolute() && p.is_file() {
        return Some(p.to_path_buf());
    }
    let names = exe_names(program);
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for n in &names {
            let c = dir.join(n);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

/// 先查 PATH；找不到再在给定目录里找（应对「刚装完 PATH 未刷新」）。
pub fn which(program: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = find_in_path(program) {
        return Some(p);
    }
    let names = exe_names(program);
    for dir in extra_dirs {
        for n in &names {
            let c = dir.join(n);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

/// 同步执行并捕获输出；超时则杀掉进程并报错。
/// 子进程输出会全量驻留内存，仅用于版本号/日志等小到中等输出。
pub fn run_capture(program: &Path, args: &[String], timeout: Duration) -> AppResult<CmdOutput> {
    use std::io::Read;
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn()
        .map_err(|e| AppError::Command(crate::tr!("无法启动 {0}: {1}", "Failed to launch {0}: {1}", program.display(), e)))?;

    // 立即接管管道并由独立线程排空：子进程写满管道会阻塞，若不读会导致假超时误杀
    let out_pipe = child.stdout.take();
    let err_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = out_pipe { let _ = p.read_to_end(&mut buf); }
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = err_pipe { let _ = p.read_to_end(&mut buf); }
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    // 不 join 读线程：孙进程可能仍持有继承的管道写端，join 会阻塞到其退出；
                    // JoinHandle 丢弃即 detach，读线程在管道 EOF 后自行结束
                    return Err(AppError::Command(crate::tr!(
                        "{0} 执行超时（{1} 秒）",
                        "{0} timed out ({1}s)",
                        program.display(),
                        timeout.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(e) => return Err(AppError::Io(e)),
        }
    };

    let stdout = out_reader.join().map_err(|_| AppError::Command(crate::tr!("读取 stdout 线程失败", "Failed to read stdout thread").into()))?;
    let stderr = err_reader.join().map_err(|_| AppError::Command(crate::tr!("读取 stderr 线程失败", "Failed to read stderr thread").into()))?;
    Ok(CmdOutput {
        code: status.code().unwrap_or(-1),
        stdout: decode_console_output(&stdout),
        stderr: decode_console_output(&stderr),
    })
}

/// 子进程输出解码：正常为 UTF-8 的程序原样解码；失败（cmd 等 console 程序在
/// 中文 Windows 上输出 GBK / 系统 ANSI 代码页）时按系统 ANSI 代码页重建，
/// 避免「不是内部或外部命令」之类报错显示成乱码。
fn decode_console_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => ansi_decode(bytes),
    }
}

#[cfg(windows)]
fn ansi_decode(bytes: &[u8]) -> String {
    use windows::Win32::Globalization::{GetACP, MultiByteToWideChar, MB_ERR_INVALID_CHARS};
    unsafe {
        let cp = GetACP();
        // 先带 MB_ERR_INVALID_CHARS 试解：字节不属于该代码页时返回 0，回退 lossy
        let len = MultiByteToWideChar(cp, MB_ERR_INVALID_CHARS, bytes, None);
        if len > 0 {
            let mut buf = vec![0u16; len as usize];
            let written = MultiByteToWideChar(cp, MB_ERR_INVALID_CHARS, bytes, Some(&mut buf));
            if written > 0 {
                return String::from_utf16_lossy(&buf[..written as usize]);
            }
        }
    }
    String::from_utf8_lossy(bytes).to_string()
}

#[cfg(not(windows))]
fn ansi_decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

/// 便捷入口：先 which，再执行。
pub fn run_program(program: &str, args: &[String], extra_dirs: &[PathBuf], timeout: Duration) -> AppResult<CmdOutput> {
    let exe = which(program, extra_dirs)
        .ok_or_else(|| AppError::Command(crate::tr!("未在 PATH 中找到「{0}」，可能尚未安装", "{0} was not found in PATH; it may not be installed", program)))?;
    run_capture(&exe, args, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_names_adds_pathext_extensions() {
        let names = exe_names("git");
        assert!(names.iter().any(|n| n.to_lowercase() == "git.exe"));
        // 裸名排在最后：Windows 优先 PATHEXT 候选，避免命中 nodejs 目录下
        // 无扩展名的 unix 脚本（如 npm）导致 os error 193
        assert_eq!(names.last().map(String::as_str), Some("git"));
        assert!(names.iter().all(|n| !n.is_empty()));
    }

    #[test]
    fn exe_names_keeps_explicit_extension() {
        let names = exe_names("foo.exe");
        assert_eq!(names, vec!["foo.exe".to_string()]);
    }

    #[test]
    fn which_resolves_npm_to_windows_executable() {
        // nodejs 目录同时存在无扩展名 npm（unix 脚本）与 npm.cmd，必须命中后者
        match find_in_path("npm") {
            Some(p) => {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                assert!(
                    ext == "cmd" || ext == "exe" || ext == "bat",
                    "npm 应解析到 Windows 可执行文件，实际 {p:?}"
                );
            }
            None => eprintln!("本机未安装 npm，跳过"),
        }
    }

    #[test]
    fn run_capture_reads_stdout() {
        let out = run_capture(Path::new("cmd"), &["/C".into(), "echo 1.2.3".into()], Duration::from_secs(10));
        let out = out.expect("cmd 应可执行");
        assert_eq!(out.code, 0);
        assert_eq!(out.first_version().as_deref(), Some("1.2.3"));
    }

    #[test]
    fn run_capture_reports_missing_program() {
        assert!(run_capture(Path::new("definitely_not_a_program_xyz"), &[], Duration::from_secs(5)).is_err());
    }

    #[test]
    fn run_capture_kills_on_timeout() {
        let t = Instant::now();
        let r = run_capture(Path::new("cmd"), &["/C".into(), "ping -n 10 127.0.0.1 >nul".into()], Duration::from_millis(300));
        assert!(r.is_err());
        assert!(t.elapsed() < Duration::from_secs(5), "应在超时后及时 kill，而不是等满 10 秒");
    }

    #[test]
    fn run_capture_handles_large_output() {
        // ~2MB 输出：管道排空修复前的实现会在此假超时
        let script = format!("for /l %i in (1,1,20000) do @echo {}", "a".repeat(100));
        let out = run_capture(Path::new("cmd"), &["/C".into(), script], Duration::from_secs(30)).expect("大输出不应超时");
        assert_eq!(out.code, 0);
        assert!(out.stdout.len() > 1_000_000);
    }

    #[test]
    fn first_version_prefers_version_lines_over_dates() {
        let out = CmdOutput { code: 0, stdout: "Built on 2026.09.24\napp version 1.2.3".into(), stderr: String::new() };
        assert_eq!(out.first_version().as_deref(), Some("1.2.3"));
    }

    #[test]
    fn find_in_path_locates_cmd() {
        assert!(find_in_path("cmd").is_some());
    }

    #[test]
    fn gbk_output_is_decoded_not_mojibake() {
        // 中文 Windows（ACP=936）下 cmd 报错输出是 GBK 字节，UTF-8 解码必然失败，
        // 应按系统代码页还原为可读文本而非乱码
        let gbk: Vec<u8> = vec![
            0x22, 0x44, 0x3A, 0x5C, 0x78, 0x22, 0x20,
            0xB2, 0xBB, 0xCA, 0xC7, // 不是
            0xC4, 0xDA, 0xB2, 0xBF, // 内部
            0xBB, 0xF2, 0xCD, 0xE2, // 或外
            0xC3, 0xFC, 0xC1, 0xEE, // 命令
        ];
        let decoded = decode_console_output(&gbk);
        let is_ansi_936 = decoded.contains("不是");
        if std::str::from_utf8(&gbk).is_ok() {
            return; // 理论不会走到：此字节序列不是合法 UTF-8
        }
        assert!(
            is_ansi_936 || decoded.contains('\u{FFFD}') == false,
            "GBK 报错应解码为可读文本，实际：{decoded}"
        );
        // UTF-8 正常输出不受影响
        assert_eq!(decode_console_output("ok 中文".as_bytes()), "ok 中文");
    }
}
