use crate::commands::assert_caller;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const HISTORY_FILE: &str = "history.json";
const HISTORY_KEY: &str = "items";
const HISTORY_MAX: usize = 1000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub url: String,
    pub title: String,
    pub visited_at: i64,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load_items(app: &AppHandle) -> Vec<HistoryItem> {
    match app.store(HISTORY_FILE) {
        Ok(store) => store
            .get(HISTORY_KEY)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_items(app: &AppHandle, items: &[HistoryItem]) {
    if let Ok(store) = app.store(HISTORY_FILE) {
        store.set(HISTORY_KEY, json!(items));
    }
}

/// content webview の遷移完了時に Rust 内部で呼ぶ。コマンドではない。
pub fn record_visit_internal(app: &AppHandle, url: &str, title: &str) {
    if url.is_empty() {
        return;
    }
    if url.starts_with("http://tauri.localhost") || url.starts_with("https://tauri.localhost") {
        return;
    }
    let mut items = load_items(app);
    // 直前と同じ URL なら重複追加しない (リロードや title 変化での連続記録を抑制)
    if let Some(last) = items.last() {
        if last.url == url {
            return;
        }
    }
    items.push(HistoryItem {
        url: url.to_string(),
        title: title.to_string(),
        visited_at: now_secs(),
    });
    if items.len() > HISTORY_MAX {
        let drop = items.len() - HISTORY_MAX;
        items.drain(0..drop);
    }
    save_items(app, &items);
}

/// 新しい順 (visited_at desc) で返す。
#[tauri::command]
pub fn list_history(
    webview: tauri::Webview,
    app: AppHandle,
) -> Result<Vec<HistoryItem>, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let mut items = load_items(&app);
    items.sort_by_key(|b| std::cmp::Reverse(b.visited_at));
    Ok(items)
}

#[tauri::command]
pub fn clear_history(webview: tauri::Webview, app: AppHandle) -> Result<(), String> {
    assert_caller(&webview, &["console"])?;
    save_items(&app, &[]);
    Ok(())
}
