// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if kokorokoe_lib::run_transcription_worker_if_requested() {
        return;
    }
    kokorokoe_lib::run()
}
