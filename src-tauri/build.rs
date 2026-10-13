fn main() {
    // tauri-build 会发出自己的 rerun-if-changed 指令，使 cargo 只监视其输入；
    // 不加这两行，修改 app.manifest 或更换图标不会触发 build.rs 重跑，
    // 内嵌清单与 exe 图标资源会停留在旧内容。
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=icons");
    // tauri-build 自带的默认清单与 embed-manifest 内嵌的清单会同时存在，
    // 导致链接期 CVT1100 (duplicate resource: MANIFEST)。
    // 这里改用 tauri-build 官方 API，用 app.manifest 替换其默认清单，只保留自建清单。
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app.manifest"),
    )
    .expect("failed to read app.manifest");
    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new()
                // 显式指定 exe 图标，保证任务栏与任务管理器使用生成的新图标
                .window_icon_path("icons/icon.ico")
                .app_manifest(manifest),
        ),
    )
    .expect("failed to run tauri-build");
}
