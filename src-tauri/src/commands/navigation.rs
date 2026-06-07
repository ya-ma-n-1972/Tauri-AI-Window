use crate::commands::assert_caller;
use crate::state::AppState;
use tauri::{AppHandle, Manager};

fn lookup_tab_webview_label(
    state: &tauri::State<'_, AppState>,
    bw_label: &str,
    tab_id: &str,
) -> Option<String> {
    let guard = state.windows.read();
    guard.get(bw_label).and_then(|bw| {
        bw.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.webview_label.clone())
    })
}

#[tauri::command]
pub async fn navigate_tab(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bw_label: String,
    tab_id: String,
    url: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let target_label = lookup_tab_webview_label(&state, &bw_label, &tab_id)
        .ok_or_else(|| format!("tab not found: {}/{}", bw_label, tab_id))?;
    let parsed: tauri::Url = url
        .parse()
        .map_err(|_| format!("invalid URL: {}", url))?;
    if let Some(wv) = app.get_webview(&target_label) {
        wv.navigate(parsed).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn reload_tab(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bw_label: String,
    tab_id: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let target_label = lookup_tab_webview_label(&state, &bw_label, &tab_id)
        .ok_or_else(|| format!("tab not found: {}/{}", bw_label, tab_id))?;
    if let Some(wv) = app.get_webview(&target_label) {
        wv.reload().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn go_back(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bw_label: String,
    tab_id: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let target_label = lookup_tab_webview_label(&state, &bw_label, &tab_id)
        .ok_or_else(|| format!("tab not found: {}/{}", bw_label, tab_id))?;
    if let Some(wv) = app.get_webview(&target_label) {
        let _ = wv.eval("history.back()");
    }
    Ok(())
}

#[tauri::command]
pub fn go_forward(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bw_label: String,
    tab_id: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let target_label = lookup_tab_webview_label(&state, &bw_label, &tab_id)
        .ok_or_else(|| format!("tab not found: {}/{}", bw_label, tab_id))?;
    if let Some(wv) = app.get_webview(&target_label) {
        let _ = wv.eval("history.forward()");
    }
    Ok(())
}
