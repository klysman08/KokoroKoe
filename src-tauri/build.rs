fn main() {
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "get_settings",
            "update_settings",
            "choose_workspace",
            "list_audio_devices",
            "start_audio_capture_prototype",
            "get_audio_capture_prototype_status",
            "stop_audio_capture_prototype",
        ]));

    tauri_build::try_build(attributes).expect("failed to build Tauri application metadata");
}
