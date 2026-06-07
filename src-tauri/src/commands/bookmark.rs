use crate::commands::assert_caller;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const BOOKMARKS_FILE: &str = "bookmarks.json";
const BOOKMARKS_KEY: &str = "items";

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkItem {
    pub url: String,
    pub title: String,
    pub added_at: i64,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 全ブックマークを読み出す。newtab 注入用に pub。
pub fn load_items(app: &AppHandle) -> Vec<BookmarkItem> {
    match app.store(BOOKMARKS_FILE) {
        Ok(store) => store
            .get(BOOKMARKS_KEY)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_items(app: &AppHandle, items: &[BookmarkItem]) -> Result<(), String> {
    let store = app.store(BOOKMARKS_FILE).map_err(|e| e.to_string())?;
    store.set(BOOKMARKS_KEY, json!(items));
    Ok(())
}

#[tauri::command]
pub fn add_bookmark(
    webview: tauri::Webview,
    app: AppHandle,
    url: String,
    title: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    if url.is_empty() {
        return Err("empty url".into());
    }
    let mut items = load_items(&app);
    if items.iter().any(|b| b.url == url) {
        return Ok(()); // 重複は無視
    }
    items.push(BookmarkItem {
        url,
        title,
        added_at: now_secs(),
    });
    save_items(&app, &items)
}

#[tauri::command]
pub fn remove_bookmark(
    webview: tauri::Webview,
    app: AppHandle,
    url: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let mut items = load_items(&app);
    let before = items.len();
    items.retain(|b| b.url != url);
    if items.len() == before {
        return Ok(()); // なかった
    }
    save_items(&app, &items)
}

/// §A.1: content (`bw_*-tab-*`=remote ページ) には公開しない。newtab のグリッドは
/// 生成時に注入する `window.__TAW_BOOKMARKS__` スナップショットを使う。
#[tauri::command]
pub fn list_bookmarks(
    webview: tauri::Webview,
    app: AppHandle,
) -> Result<Vec<BookmarkItem>, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    Ok(load_items(&app))
}

#[tauri::command]
pub fn is_bookmarked(
    webview: tauri::Webview,
    app: AppHandle,
    url: String,
) -> Result<bool, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let items = load_items(&app);
    Ok(items.iter().any(|b| b.url == url))
}
