use crate::commands::assert_caller;
use crate::commands::tab::build_content_webview;
use crate::state::{
    AppState, BrowserWindow, BrowserWindowSummary, LinkOpenMode, Tab, DEFAULT_PROFILE_ID,
    OFFSCREEN_Y, TABBAR_HEIGHT,
};
use serde_json::json;
use tauri::webview::WebviewBuilder;
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, PhysicalSize,
    WebviewUrl, WindowEvent,
};

const INITIAL_WIDTH: f64 = 1200.0;
const INITIAL_HEIGHT: f64 = 800.0;

#[tauri::command]
pub async fn new_browser_window(
    webview: tauri::Webview,
    app: AppHandle,
    initial_url: Option<String>,
    profile_id: Option<String>,
) -> Result<String, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let profile = profile_id.unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string());
    open_browser_window_internal(&app, initial_url, profile).await
}

/// 新規 BW を作る内部関数。assert_caller 抜きなので、コマンド層 (`new_browser_window`)
/// と link 層 (`report_link_action` / `on_new_window`) の両方から呼び出される。
pub(crate) async fn open_browser_window_internal(
    app: &AppHandle,
    initial_url: Option<String>,
    profile_id: String,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let bw_label = state.alloc_bw_label();
    let tabbar_label = format!("{}-tabbar", bw_label);

    let window = WindowBuilder::new(app, &bw_label)
        .title("Tauri AI Window")
        .inner_size(INITIAL_WIDTH, INITIAL_HEIGHT)
        .build()
        .map_err(|e| e.to_string())?;

    // tabbar
    window
        .add_child(
            WebviewBuilder::new(&tabbar_label, WebviewUrl::App("tabbar.html".into())),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(INITIAL_WIDTH, TABBAR_HEIGHT),
        )
        .map_err(|e| e.to_string())?;

    // 1 つ目のタブ。initial_url が None ならローカル newtab.html、Some なら外部 URL。
    let tab_id = state.alloc_tab_id();
    let tab_webview_label = format!("{}-tab-{}", bw_label, tab_id);
    let is_newtab = initial_url.is_none();
    let (state_url, webview_url) = match &initial_url {
        Some(u) => (
            u.clone(),
            WebviewUrl::External(u.parse().map_err(|_| format!("invalid URL: {}", u))?),
        ),
        None => (String::new(), WebviewUrl::App("newtab.html".into())),
    };

    window
        .add_child(
            build_content_webview(
                app,
                &bw_label,
                &tab_id,
                tab_webview_label.clone(),
                webview_url,
                &profile_id,
                is_newtab,
            ),
            LogicalPosition::new(0.0, TABBAR_HEIGHT),
            LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT - TABBAR_HEIGHT),
        )
        .map_err(|e| e.to_string())?;

    // AppState 更新
    {
        let mut windows = state.windows.write();
        windows.insert(
            bw_label.clone(),
            BrowserWindow {
                label: bw_label.clone(),
                tabbar_label: tabbar_label.clone(),
                tabs: vec![Tab {
                    id: tab_id.clone(),
                    webview_label: tab_webview_label.clone(),
                    title: String::new(),
                    url: state_url.clone(),
                }],
                active_tab_id: Some(tab_id.clone()),
                profile_id: profile_id.clone(),
                link_open_mode: LinkOpenMode::Tab,
            },
        );
    }

    // window event 配線
    let app_for_event = app.clone();
    let bw_for_event = bw_label.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Resized(size) => {
            handle_resize(&app_for_event, &bw_for_event, *size);
        }
        WindowEvent::CloseRequested { .. } => {
            handle_close(&app_for_event, &bw_for_event);
        }
        _ => {}
    });

    let _ = app.emit_to(
        EventTarget::webview("console"),
        "bw://opened",
        json!({ "bwLabel": bw_label }),
    );

    Ok(bw_label)
}

fn handle_resize(app: &AppHandle, bw_label: &str, size: PhysicalSize<u32>) {
    let scale = app
        .get_window(bw_label)
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    let width = size.width as f64 / scale;
    let height = size.height as f64 / scale;
    let content_height = (height - TABBAR_HEIGHT).max(0.0);

    let tabbar_label = format!("{}-tabbar", bw_label);
    if let Some(wv) = app.get_webview(&tabbar_label) {
        let _ = wv.set_size(LogicalSize::new(width, TABBAR_HEIGHT));
    }

    let state = app.state::<AppState>();
    let (active_tab_label, all_tab_labels): (Option<String>, Vec<String>) = {
        let guard = state.windows.read();
        if let Some(bw) = guard.get(bw_label) {
            let active = bw.active_tab_id.as_ref().and_then(|id| {
                bw.tabs
                    .iter()
                    .find(|t| &t.id == id)
                    .map(|t| t.webview_label.clone())
            });
            let all: Vec<String> = bw.tabs.iter().map(|t| t.webview_label.clone()).collect();
            (active, all)
        } else {
            (None, vec![])
        }
    };

    for tab_label in &all_tab_labels {
        if let Some(wv) = app.get_webview(tab_label) {
            let _ = wv.set_size(LogicalSize::new(width, content_height));
            // active のみ位置を再設定。非 active は OFFSCREEN_Y のまま。
            if Some(tab_label) == active_tab_label.as_ref() {
                let _ = wv.set_position(LogicalPosition::new(0.0, TABBAR_HEIGHT));
            } else {
                let _ = wv.set_position(LogicalPosition::new(0.0, OFFSCREEN_Y));
            }
        }
    }
}

fn handle_close(app: &AppHandle, bw_label: &str) {
    {
        let state = app.state::<AppState>();
        let mut windows = state.windows.write();
        windows.remove(bw_label);
    }
    let _ = app.emit_to(
        EventTarget::webview("console"),
        "bw://closed",
        json!({ "bwLabel": bw_label }),
    );
}

#[tauri::command]
pub async fn close_browser_window(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bw_label: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;

    let (tab_labels, tabbar_label) = {
        let guard = state.windows.read();
        match guard.get(&bw_label) {
            Some(bw) => (
                bw.tabs
                    .iter()
                    .map(|t| t.webview_label.clone())
                    .collect::<Vec<_>>(),
                bw.tabbar_label.clone(),
            ),
            None => return Err(format!("BW not found: {}", bw_label)),
        }
    };

    // 子 tab → tabbar → window の順で close。
    for label in &tab_labels {
        if let Some(wv) = app.get_webview(label) {
            let _ = wv.close();
        }
    }
    if let Some(wv) = app.get_webview(&tabbar_label) {
        let _ = wv.close();
    }
    if let Some(window) = app.get_window(&bw_label) {
        // window.close() が CloseRequested を発火させ、handle_close で AppState 整理 + emit が走る。
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
pub fn focus_browser_window(
    webview: tauri::Webview,
    app: AppHandle,
    bw_label: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console"])?;
    if let Some(window) = app.get_window(&bw_label) {
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_browser_windows(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<BrowserWindowSummary>, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let guard = state.windows.read();
    Ok(guard
        .values()
        .map(|bw| BrowserWindowSummary {
            label: bw.label.clone(),
            tab_count: bw.tabs.len(),
            active_tab_id: bw.active_tab_id.clone(),
            profile_id: bw.profile_id.clone(),
        })
        .collect())
}

/// §2.5: BW のリンク開きモード (既定の開き方) を設定する。tabbar のトグルから呼ばれる。
/// 永続化はしない (AppState のみ)。
#[tauri::command]
pub fn set_link_open_mode(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
    bw_label: String,
    mode: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let m = match mode.as_str() {
        "tab" => LinkOpenMode::Tab,
        "window" => LinkOpenMode::Window,
        _ => return Err(format!("invalid mode: {}", mode)),
    };
    let mut windows = state.windows.write();
    let bw = windows
        .get_mut(&bw_label)
        .ok_or_else(|| format!("BW not found: {}", bw_label))?;
    bw.link_open_mode = m;
    Ok(())
}

/// §2.5: BW のリンク開きモードを取得する。tabbar 起動時の初期表示に使う。
/// BW 未登録時 (tabbar webview が windows.insert より先に起動した場合) は既定の "tab" を返す。
#[tauri::command]
pub fn get_link_open_mode(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
    bw_label: String,
) -> Result<String, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let guard = state.windows.read();
    let mode = guard
        .get(&bw_label)
        .map(|bw| bw.link_open_mode)
        .unwrap_or(LinkOpenMode::Tab);
    Ok(match mode {
        LinkOpenMode::Tab => "tab",
        LinkOpenMode::Window => "window",
    }
    .to_string())
}
