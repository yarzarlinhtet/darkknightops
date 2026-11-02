fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_servers",
            "get_server",
            "create_server",
            "update_server",
            "delete_server",
        ]),
    ))
    .expect("Tauri build configuration must be valid");
}
