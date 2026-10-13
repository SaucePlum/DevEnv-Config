//! 注册表环境变量读写、写入前检测、备份与广播生效。
//! 设计依据：设计文档第 11 节。核心是「写前必检」与「规范化比较」。

use crate::download;
use crate::env_norm;
use crate::error::{AppError, AppResult};
use crate::manifest::Scope;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// PATH 写入结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathWrite {
    /// 已存在等价条目，跳过（多由安装器自行配置）
    Skipped,
    /// 追加了新条目
    Appended,
}

fn map_reg(e: std::io::Error) -> AppError {
    AppError::Registry(e.to_string())
}

/// 单个环境变量值的原始快照：注册表类型 + 原始字节（hex 编码）。
/// 保留原始类型是为了恢复时原样写回——PATH/TEMP 等是 REG_EXPAND_SZ，
/// 若降级为 REG_SZ，`%SystemRoot%` 等变量引用将不再展开。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RawEnvValue {
    /// winreg RegType 的 i32 内部值（REG_SZ=1，REG_EXPAND_SZ=2 等）
    vtype: i32,
    /// RegValue.bytes 的 hex 编码（UTF-16LE 含 NUL 结尾）
    hex: String,
}

/// winreg 0.52 的 RegType 是 fieldless enum，只能按判别值映射重建。
fn reg_type_from_i32(v: i32) -> Option<winreg::enums::RegType> {
    use winreg::enums::RegType::*;
    match v {
        0 => Some(REG_NONE),
        1 => Some(REG_SZ),
        2 => Some(REG_EXPAND_SZ),
        3 => Some(REG_BINARY),
        4 => Some(REG_DWORD),
        5 => Some(REG_DWORD_BIG_ENDIAN),
        6 => Some(REG_LINK),
        7 => Some(REG_MULTI_SZ),
        8 => Some(REG_RESOURCE_LIST),
        9 => Some(REG_FULL_RESOURCE_DESCRIPTOR),
        10 => Some(REG_RESOURCE_REQUIREMENTS_LIST),
        11 => Some(REG_QWORD),
        _ => None,
    }
}

/// 读取 PATH 原始字符串（不做变量展开）。
pub fn read_path(scope: Scope) -> AppResult<String> {
    use winreg::enums::*;
    let (key, name) = match scope {
        Scope::User => (
            winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE),
            "Path".to_string(),
        ),
        Scope::System => (
            winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", KEY_READ | KEY_WRITE),
            "Path".to_string(),
        ),
    };
    let key = key.map_err(map_reg)?;
    match key.get_value::<String, _>(&name) {
        Ok(v) => Ok(v),
        Err(_) => Ok(String::new()),
    }
}

/// 写入 PATH（保持 REG_EXPAND_SZ 类型，变量引用不被写坏）。
pub fn write_path(scope: Scope, value: &str) -> AppResult<()> {
    write_string(scope, "Path", value, true)
}

/// 读取普通变量。
pub fn read_var(scope: Scope, name: &str) -> Option<String> {
    let key = match scope {
        Scope::User => winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", winreg::enums::KEY_READ)
            .ok()?,
        Scope::System => winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", winreg::enums::KEY_READ)
            .ok()?,
    };
    key.get_value::<String, _>(name).ok()
}

pub fn set_var(scope: Scope, name: &str, value: &str) -> AppResult<()> {
    write_string(scope, name, value, false)
}

fn write_string(scope: Scope, name: &str, value: &str, expand: bool) -> AppResult<()> {
    use winreg::enums::*;
    use winreg::RegValue;
    let key = match scope {
        Scope::User => winreg::RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .map_err(map_reg)?,
        Scope::System => winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", KEY_READ | KEY_WRITE)
            .map_err(map_reg)?,
    };
    let mut bytes = Vec::with_capacity(value.len() * 2 + 2);
    for u in value.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0]);
    let vtype = if expand { RegType::REG_EXPAND_SZ } else { RegType::REG_SZ };
    key.set_raw_value(name, &RegValue { bytes, vtype }).map_err(map_reg)?;
    Ok(())
}

/// 确保 PATH 中存在某目录；已存在（规范化比较）则跳过。
pub fn ensure_path_entry(scope: Scope, dir: &str) -> AppResult<PathWrite> {
    let current = read_path(scope)?;
    match env_norm::path_append_value(&current, dir) {
        None => Ok(PathWrite::Skipped),
        Some(next) => {
            let backup = backup_env(&crate::tr!("修改 PATH 前", "Before PATH change"))?;
            if let Err(e) = write_path(scope, &next) {
                // 写入失败：回滚到备份值，备份文件保留在备份目录
                let name = backup.file_name().and_then(|s| s.to_str()).unwrap_or_default();
                let _ = restore_backup(name);
                return Err(e);
            }
            broadcast_change();
            Ok(PathWrite::Appended)
        }
    }
}

/// 广播 WM_SETTINGCHANGE，让新开的终端看到改动。
pub fn broadcast_change() {
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    let param: Vec<u16> = "Environment".encode_utf16().chain(std::iter::once(0)).collect();
    let mut result = 0usize;
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(param.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            Some(&mut result),
        );
    }
}

/// 备份目录：`%LOCALAPPDATA%\DevEnv\env-backup`
pub fn backup_dir() -> PathBuf {
    download::data_dir().join("env-backup")
}

/// 把当前用户级 / 系统级 Environment 快照（原始类型+字节）写入 JSON 备份文件，返回文件路径。
pub fn backup_env(label: &str) -> AppResult<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;

    let mut snapshot: BTreeMap<String, BTreeMap<String, RawEnvValue>> = BTreeMap::new();
    let mut user = BTreeMap::new();
    let mut system = BTreeMap::new();

    // enum_values() 迭代项为 (名称, RegValue)：直接拿原始字节与类型，不做字符串化
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", KEY_READ) {
        for (name, rv) in k.enum_values().flatten() {
            user.insert(name, RawEnvValue { vtype: rv.vtype as i32, hex: hex::encode(&rv.bytes) });
        }
    }
    if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", KEY_READ)
    {
        for (name, rv) in k.enum_values().flatten() {
            system.insert(name, RawEnvValue { vtype: rv.vtype as i32, hex: hex::encode(&rv.bytes) });
        }
    }
    snapshot.insert("user".into(), user);
    snapshot.insert("system".into(), system);

    std::fs::create_dir_all(backup_dir())?;
    let fmt = if crate::i18n::is_en() { "%Y-%m-%d %H%M%S" } else { "%Y年%m月%d日 %H%M%S" };
    let ts = chrono::Local::now().format(fmt).to_string();
    let path = backup_dir().join(format!("{label}-{ts}.json"));
    let json = serde_json::to_string_pretty(&snapshot).map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

pub fn list_backups() -> AppResult<Vec<String>> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(backup_dir()) {
        for e in rd.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if name.ends_with(".json") {
                    out.push(name.to_string());
                }
            }
        }
    }
    out.sort();
    out.reverse();
    Ok(out)
}

/// 校验备份文件名（防路径逃逸）并返回完整路径。
fn backup_path(file_name: &str) -> AppResult<PathBuf> {
    if file_name.contains('\\') || file_name.contains('/') || file_name.contains("..") {
        return Err(AppError::Config(crate::tr!("非法的备份文件名", "Invalid backup file name").into()));
    }
    Ok(backup_dir().join(file_name))
}

/// 读取备份文件内容（JSON 快照）。
pub fn read_backup(file_name: &str) -> AppResult<String> {
    let path = backup_path(file_name)?;
    std::fs::read_to_string(&path).map_err(|e| AppError::Config(crate::tr!("读取备份失败: {0}", "Failed to read backup: {0}", e)))
}

/// 删除备份文件。
pub fn delete_backup(file_name: &str) -> AppResult<()> {
    let path = backup_path(file_name)?;
    std::fs::remove_file(&path).map_err(|e| AppError::Config(crate::tr!("删除备份失败: {0}", "Failed to delete backup: {0}", e)))
}

/// 从备份文件恢复用户级与系统级 Environment（原始类型+字节原样写回，不降级 REG_EXPAND_SZ）。
/// 单个值失败不中断（部分恢复优于全有全无）；全部失败时报错。
pub fn restore_backup(file_name: &str) -> AppResult<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let path = backup_path(file_name)?;
    let text = std::fs::read_to_string(&path)?;
    let snapshot: BTreeMap<String, BTreeMap<String, RawEnvValue>> =
        serde_json::from_str(&text).map_err(|e| AppError::Config(e.to_string()))?;

    let mut ok = 0usize;
    let mut fail = 0usize;

    if let Some(user) = snapshot.get("user") {
        match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE) {
            Ok(key) => {
                for (name, raw) in user {
                    if write_raw_value(&key, name, raw).is_ok() {
                        ok += 1;
                    } else {
                        fail += 1;
                    }
                }
            }
            Err(_) => fail += user.len(),
        }
    }
    if let Some(system) = snapshot.get("system") {
        match RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", KEY_READ | KEY_WRITE)
        {
            Ok(key) => {
                for (name, raw) in system {
                    if write_raw_value(&key, name, raw).is_ok() {
                        ok += 1;
                    } else {
                        fail += 1;
                    }
                }
            }
            Err(_) => fail += system.len(),
        }
    }
    broadcast_change();
    if ok == 0 && fail > 0 {
        return Err(AppError::Registry(crate::tr!("环境变量恢复失败：{0} 个值全部写入失败", "Failed to restore environment variables: all {0} values failed to write", fail)));
    }
    Ok(())
}

/// 把原始快照值（hex 解码 + 类型重建）写回注册表键。
fn write_raw_value(key: &winreg::RegKey, name: &str, raw: &RawEnvValue) -> AppResult<()> {
    let bytes = hex::decode(&raw.hex).map_err(|e| AppError::Config(crate::tr!("备份值 hex 解码失败: {0}", "Failed to hex-decode backup value: {0}", e)))?;
    let vtype = reg_type_from_i32(raw.vtype)
        .ok_or_else(|| AppError::Config(crate::tr!("未知注册表类型: {0}", "Unknown registry type: {0}", raw.vtype)))?;
    key.set_raw_value(name, &winreg::RegValue { bytes, vtype })
        .map_err(map_reg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_dir_is_under_data_dir() {
        assert!(backup_dir().ends_with("env-backup"));
    }

    #[test]
    fn read_user_path_does_not_panic() {
        // 只验证可读取（不写注册表，避免污染环境）
        let _ = read_path(Scope::User);
    }

    #[test]
    fn list_backups_returns_sorted_desc() {
        let list = list_backups().unwrap_or_default();
        let mut sorted = list.clone();
        sorted.sort();
        sorted.reverse();
        assert_eq!(list, sorted);
    }

    #[test]
    fn raw_env_value_hex_roundtrip() {
        let v = RawEnvValue { vtype: 2, hex: hex::encode("abc".as_bytes()) };
        let decoded = hex::decode(&v.hex).unwrap();
        assert_eq!(decoded, b"abc");
        assert_eq!(v.vtype, 2);
    }

    #[test]
    fn reg_type_i32_roundtrip_covers_all_known_types() {
        // REG_EXPAND_SZ 必须映射回 2，否则恢复时会降级类型
        for v in 0..=11i32 {
            let t = reg_type_from_i32(v).expect("0..=11 均应为已知 RegType");
            assert_eq!(t as i32, v);
        }
        assert!(reg_type_from_i32(99).is_none());
    }
}
