use crate::commands::assert_caller;
use crate::state::{AppState, DEFAULT_PROFILE_ID};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const PROFILES_FILE: &str = "profiles.json";
const PROFILES_KEY: &str = "items";

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProfileItem {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_nanos() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
}

fn generate_id() -> String {
    format!("p{}_{:09}", now_secs(), now_nanos())
}

fn load(app: &AppHandle) -> Vec<ProfileItem> {
    match app.store(PROFILES_FILE) {
        Ok(store) => store
            .get(PROFILES_KEY)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save(app: &AppHandle, items: &[ProfileItem]) -> Result<(), String> {
    let store = app.store(PROFILES_FILE).map_err(|e| e.to_string())?;
    store.set(PROFILES_KEY, json!(items));
    Ok(())
}

fn ensure_default(app: &AppHandle) -> Vec<ProfileItem> {
    let mut items = load(app);
    if !items.iter().any(|p| p.id == DEFAULT_PROFILE_ID) {
        items.insert(
            0,
            ProfileItem {
                id: DEFAULT_PROFILE_ID.into(),
                name: "デフォルト".into(),
                created_at: now_secs(),
            },
        );
        let _ = save(app, &items);
    }
    items
}

#[tauri::command]
pub fn list_profiles(
    webview: tauri::Webview,
    app: AppHandle,
) -> Result<Vec<ProfileItem>, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    Ok(ensure_default(&app))
}

#[tauri::command]
pub fn add_profile(
    webview: tauri::Webview,
    app: AppHandle,
    name: String,
) -> Result<ProfileItem, String> {
    assert_caller(&webview, &["console"])?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name is empty".into());
    }
    let mut items = ensure_default(&app);
    let item = ProfileItem {
        id: generate_id(),
        name: trimmed.to_string(),
        created_at: now_secs(),
    };
    items.push(item.clone());
    save(&app, &items)?;
    Ok(item)
}

#[tauri::command]
pub fn remove_profile(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console"])?;
    if id == DEFAULT_PROFILE_ID {
        return Err("cannot remove default profile".into());
    }
    let in_use = {
        let guard = state.windows.read();
        guard.values().any(|bw| bw.profile_id == id)
    };
    if in_use {
        return Err("profile is in use by an open window".into());
    }
    let mut items = load(&app);
    items.retain(|p| p.id != id);
    save(&app, &items)
}
