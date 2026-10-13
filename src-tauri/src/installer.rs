//! 调起安装包并等待其退出。不传任何静默参数，向导由用户完成。
//! 用 `ShellExecuteExW` 拿到进程句柄，再 `WaitForSingleObject` 等待退出。

use crate::error::{AppError, AppResult};
use std::path::Path;

/// 运行期权限自检：进程是否以管理员身份运行。
pub fn is_elevated() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 调起 `file` 并阻塞等待退出，返回退出码。应在阻塞线程上调用。
pub fn run_and_wait(file: &Path, params: Option<&str>) -> AppResult<i32> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    // ShellExecuteExW 可能委托 shell 扩展（如 MSI 处理程序），要求线程已初始化 COM。
    // 失败忽略（本线程可能已初始化过，RPC_E_CHANGED_MODE 等）；不配对 CoUninitialize，
    // 由进程/线程生命周期统一回收。
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    if !file.exists() {
        return Err(AppError::MissingResource(file.display().to_string()));
    }

    let file_w = to_wide(&file.to_string_lossy());
    let params_w = params.map(to_wide);

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: match &params_w {
            Some(p) => PCWSTR(p.as_ptr()),
            None => PCWSTR::null(),
        },
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    unsafe {
        ShellExecuteExW(&mut sei)
            .map_err(|e| AppError::Command(crate::tr!("无法启动 {0}: {1}", "Failed to launch {0}: {1}", file.display(), e)))?;
        if sei.hProcess.is_invalid() {
            return Err(AppError::Command(crate::tr!("未取得进程句柄: {0}", "Failed to obtain process handle: {0}", file.display())));
        }
        let wait = WaitForSingleObject(sei.hProcess, INFINITE);
        if wait != windows::Win32::Foundation::WAIT_OBJECT_0 {
            return Err(AppError::Command(crate::tr!(
                "等待 {0} 退出失败（WAIT 状态 {1}）",
                "Failed waiting for {0} to exit (WAIT status {1})",
                file.display(),
                wait.0
            )));
        }
        let mut code: u32 = 0;
        GetExitCodeProcess(sei.hProcess, &mut code)
            .map_err(|e| AppError::Command(crate::tr!("读取退出码失败: {0}", "Failed to read exit code: {0}", e)))?;
        let _ = CloseHandle(sei.hProcess);
        Ok(code as i32)
    }
}

/// 用系统默认处理程序打开 URL 或文件（不等待、不取进程句柄）。
/// 仅接受 https，避免运行时任意地址/命令派发。
pub fn open_externally(target: &str) -> AppResult<()> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    if !target.starts_with("https://") {
        return Err(AppError::Config(crate::tr!("只允许打开 https 地址", "Only https URLs are allowed").into()));
    }

    // 与 run_and_wait 相同：ShellExecuteExW 可能委托 shell 扩展，先确保 COM 已初始化。
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let target_w = to_wide(target);
    let verb_w = to_wide("open");
    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb_w.as_ptr()),
        lpFile: PCWSTR(target_w.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut sei)
            .map_err(|e| AppError::Command(crate::tr!("无法打开 {0}: {1}", "Failed to open {0}: {1}", target, e)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_rejected() {
        let r = run_and_wait(Path::new("C:\\no\\such\\setup.exe"), None);
        assert!(r.is_err());
    }

    #[test]
    fn elevation_probe_returns_bool() {
        // 只验证可调用，不做断言（测试环境可能非管理员）
        let _ = is_elevated();
    }
}
