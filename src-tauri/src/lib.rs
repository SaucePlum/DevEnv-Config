pub mod commands;
pub mod config;
pub mod deploy;
pub mod detect;
pub mod download;
pub mod env;
pub mod env_norm;
pub mod error;
pub mod http;
pub mod installer;
pub mod manifest;
pub mod payload;
pub mod proc;
pub mod provider;
pub mod state;
pub mod verify;
pub mod xml;

#[cfg(test)]
mod manifest_tests;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if !installer::is_elevated() {
                state::log(app.handle(), "warn", None, "当前不是管理员权限，安装与系统级环境变量写入可能失败");
            }
            let manifest = manifest::Manifest::load().map_err(|e| format!("清单加载失败: {e}"))?;
            app.manage(commands::AppState { manifest });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_items,
            commands::list_versions,
            commands::start_item,
            commands::run_config,
            commands::is_elevated,
            commands::config_defaults,
            commands::dir_needs_confirm,
            commands::read_public_key,
            commands::list_ssh_public_keys,
            commands::ssh_key_status,
            commands::copy_text,
            commands::open_url,
            commands::test_ssh,
            commands::list_env_backups,
            commands::read_env_backup,
            commands::delete_env_backup,
            commands::list_cache_entries,
            commands::delete_cache_entry,
            commands::clear_cache,
            commands::restore_env_backup,
            commands::cache_size_mb,
            commands::open_cache_dir,
            commands::app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
