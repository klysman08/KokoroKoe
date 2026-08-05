mod commands;
mod domain;
mod logging;
mod security;

pub const PRODUCT_NAME: &str = "KokoroKoe";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::settings::get_settings])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::PRODUCT_NAME;

    #[test]
    fn product_name_is_stable() {
        assert_eq!(PRODUCT_NAME, "KokoroKoe");
    }
}
