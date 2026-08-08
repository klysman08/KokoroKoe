fn main() {
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "get_settings",
            "update_settings",
            "choose_workspace",
        ]));

    tauri_build::try_build(attributes).expect("failed to build Tauri application metadata");
}
