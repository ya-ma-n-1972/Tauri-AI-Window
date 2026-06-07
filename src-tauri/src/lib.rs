mod commands;
mod inject_scripts;
mod state;

use state::AppState;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            // Console は管制塔。閉じる = アプリ全体終了 とする。
            // tauri.conf.json で static 宣言されているのでこの時点で取得可能。
            if let Some(console) = app.get_window("console") {
                let h = app.handle().clone();
                console.on_window_event(move |event| {
                    if matches!(event, WindowEvent::CloseRequested { .. }) {
                        let labels: Vec<String> = {
                            let state = h.state::<AppState>();
                            let guard = state.windows.read();
                            guard.keys().cloned().collect()
                        };
                        for label in labels {
                            if let Some(w) = h.get_window(&label) {
                                let _ = w.close();
                            }
                        }
                        h.cleanup_before_exit();
                        h.exit(0);
                    }
                });
            } else {
                eprintln!("[lib.rs] WARN: console window not found at setup");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::window::new_browser_window,
            commands::window::close_browser_window,
            commands::window::focus_browser_window,
            commands::window::list_browser_windows,
            commands::window::set_link_open_mode,
            commands::window::get_link_open_mode,
            commands::tab::new_tab,
            commands::tab::close_tab,
            commands::tab::switch_tab,
            commands::tab::list_tabs,
            commands::tab::report_url_change,
            commands::navigation::navigate_tab,
            commands::navigation::reload_tab,
            commands::navigation::go_back,
            commands::navigation::go_forward,
            commands::link::report_link_action,
            commands::download::open_download_file,
            commands::download::open_download_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
