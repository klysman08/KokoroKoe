fn main() {
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "get_settings",
            "update_settings",
            "choose_workspace",
            "open_workspace_folder",
            "list_audio_devices",
            "start_audio_device_test",
            "stop_audio_device_test",
            "list_transcription_models",
            "download_transcription_model",
            "cancel_model_download",
            "resume_model_download",
            "delete_transcription_model",
            "set_default_transcription_model",
            "ask_manual_question",
            "generate_recent_insights",
            "get_session_summary",
            "generate_session_summary",
            "open_transcript_window",
            "close_transcript_window",
            "get_transcript_window_appearance",
            "set_transcript_window_appearance",
            "get_transcript_window_shortcut",
            "set_transcript_window_shortcut",
            "get_transcript_window_interaction",
            "set_transcript_window_interaction",
            "open_insights_window",
            "close_insights_window",
        ]));

    tauri_build::try_build(attributes).expect("failed to build Tauri application metadata");
}
