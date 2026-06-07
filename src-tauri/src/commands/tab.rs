use crate::commands::assert_caller;
use crate::state::{
    AppState, LinkOpenMode, Tab, TabSummary, DEFAULT_PROFILE_ID, OFFSCREEN_Y, TABBAR_HEIGHT,
};
use serde_json::json;
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, WebviewUrl};

/// content webview を組み立てる共通関数。on_navigation / on_page_load /
/// on_document_title_changed / on_new_window を配線して、URL・タイトル・ロード状況の
/// 変化を AppState と Tabbar に同期させ、`window.open`/`target=_blank` を横取りする。
/// new_browser_window と new_tab の両方から呼ぶ。
///
/// `is_newtab`=true (空タブ＝newtab.html) のときは、ブックマークのスナップショットを
/// `window.__TAW_BOOKMARKS__` として注入する (§A.1: content 用 list_bookmarks を公開しない代替)。
pub fn build_content_webview(
    app: &AppHandle,
    bw_label: &str,
    tab_id: &str,
    webview_label: String,
    url: WebviewUrl,
    profile_id: &str,
    is_newtab: bool,
) -> WebviewBuilder<tauri::Wry> {
    // プロファイル別の data_directory を作成。Windows の WebView2 はこの dir を
    // User Data Folder として使い、Cookie/localStorage 等が分離される。
    let data_dir: Option<std::path::PathBuf> = match app.path().app_local_data_dir() {
        Ok(p) => {
            let dir = p.join("profiles").join(profile_id);
            let _ = std::fs::create_dir_all(&dir);
            Some(dir)
        }
        Err(_) => None,
    };
    let app_for_nav = app.clone();
    let bw_for_nav = bw_label.to_string();
    let tab_for_nav = tab_id.to_string();
    let app_for_load = app.clone();
    let bw_for_load = bw_label.to_string();
    let tab_for_load = tab_id.to_string();
    let app_for_title = app.clone();
    let bw_for_title = bw_label.to_string();
    let tab_for_title = tab_id.to_string();
    let app_for_new = app.clone();
    let bw_for_new = bw_label.to_string();
    let app_for_dl = app.clone();
    let app_for_visit = app.clone();
    let bw_for_visit = bw_label.to_string();
    let tab_for_visit = tab_id.to_string();

    let mut init_script = format!(
        "{}\n;\n{}",
        crate::inject_scripts::URL_WATCH_JS,
        crate::inject_scripts::LINK_INTERCEPT_JS
    );

    // §A.1 セキュア: newtab のグリッド用にブックマークのスナップショットを注入する。
    // `tauri.localhost` ガードにより、この webview が後で remote URL へ遷移しても
    // remote ページにはグローバルが設定されない (漏洩しない)。
    if is_newtab {
        let bookmarks = crate::commands::bookmark::load_items(app);
        let bm_json = serde_json::to_string(&bookmarks).unwrap_or_else(|_| "[]".to_string());
        init_script.push_str(&format!(
            "\n;\nif(window.location.host==='tauri.localhost'){{window.__TAW_BOOKMARKS__={};}}",
            bm_json
        ));
    }

    // §2.1: Tauri の D&D 横取りを止め、WebView2 のネイティブ HTML5 drop をページに通す
    // (外部ファイル → ページ内のアップロード)。Windows で HTML5 D&D を使うのに必要。
    // 副作用: この webview で Tauri の onDragDropEvent は発火しなくなるが、content では未使用。
    let mut builder = WebviewBuilder::new(webview_label, url)
        .initialization_script_for_all_frames(init_script)
        .disable_drag_drop_handler();
    if let Some(d) = data_dir {
        builder = builder.data_directory(d);
    }
    builder
        .on_navigation(move |u| {
            update_tab_url(&app_for_nav, &bw_for_nav, &tab_for_nav, u.as_str());
            true
        })
        .on_page_load(move |webview, payload| {
            let event_name = match payload.event() {
                PageLoadEvent::Started => "tab://load-started",
                PageLoadEvent::Finished => "tab://load-finished",
            };
            // Finished で webview.url() を読んで AppState 同期 (Windows の on_webview_ready
            // 直後 url() が空文字を返すバグへの二重保険)。
            if matches!(payload.event(), PageLoadEvent::Finished) {
                if let Ok(u) = webview.url() {
                    update_tab_url(&app_for_load, &bw_for_load, &tab_for_load, u.as_str());
                }
                // 履歴記録: 現タブの url/title を AppState から取って store へ。
                let (rec_url, rec_title) = {
                    let state = app_for_visit.state::<AppState>();
                    let guard = state.windows.read();
                    guard
                        .get(&bw_for_visit)
                        .and_then(|bw| bw.tabs.iter().find(|t| t.id == tab_for_visit))
                        .map(|t| (t.url.clone(), t.title.clone()))
                        .unwrap_or_default()
                };
                if !rec_url.is_empty() {
                    crate::commands::history::record_visit_internal(
                        &app_for_visit,
                        &rec_url,
                        &rec_title,
                    );
                }
            }
            let _ = app_for_load.emit_to(
                EventTarget::webview(format!("{}-tabbar", bw_for_load)),
                event_name,
                json!({ "bwLabel": bw_for_load, "tabId": tab_for_load }),
            );
        })
        .on_document_title_changed(move |_w, title| {
            update_tab_title(&app_for_title, &bw_for_title, &tab_for_title, &title);
            let _ = app_for_title.emit_to(
                EventTarget::webview(format!("{}-tabbar", bw_for_title)),
                "tab://title-changed",
                json!({ "bwLabel": bw_for_title, "tabId": tab_for_title, "title": title }),
            );
        })
        .on_new_window(move |url, features| {
            // window.open(url, target, features) を捕捉。Tauri 自動生成は Deny し、自前で開く。
            // §2.5 優先順位: (2) popup features 明示 → 新規ウィンドウ (OAuth/認証小窓を壊さない)、
            //               (3) 無ければ BW のリンク開きモード (スイッチ) に従う。
            // ※ 修飾キー (優先順位1) は window.open には乗らないため、ここでは扱わない。
            let has_popup_features = features.size().is_some() || features.position().is_some();
            let app_clone = app_for_new.clone();
            let bw_clone = bw_for_new.clone();
            let bw_for_lookup = bw_for_new.clone();
            let url_string = url.to_string();
            // tauri::async_runtime::spawn 経由で別スレッド (Tauri 内部 tokio) に乗せる。
            // on_new_window 同期コンテキストから直接 add_child を呼ぶと Windows でデッドロック (§1-3)。
            tauri::async_runtime::spawn(async move {
                let to_window = if has_popup_features {
                    true
                } else {
                    let state = app_clone.state::<AppState>();
                    let guard = state.windows.read();
                    guard
                        .get(&bw_for_lookup)
                        .map(|bw| bw.link_open_mode == LinkOpenMode::Window)
                        .unwrap_or(false)
                };
                let r = if to_window {
                    // 親 BW の profile_id を継承
                    let parent_profile = {
                        let state = app_clone.state::<AppState>();
                        let guard = state.windows.read();
                        guard
                            .get(&bw_for_lookup)
                            .map(|bw| bw.profile_id.clone())
                            .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string())
                    };
                    crate::commands::window::open_browser_window_internal(
                        &app_clone,
                        Some(url_string),
                        parent_profile,
                    )
                    .await
                    .map(|_| ())
                } else {
                    crate::commands::tab::open_tab_internal(&app_clone, bw_clone, url_string, true)
                        .await
                        .map(|_| ())
                };
                if let Err(e) = r {
                    eprintln!("[on_new_window] failed: {}", e);
                }
            });
            tauri::webview::NewWindowResponse::Deny
        })
        .on_download(move |_w, event| {
            crate::commands::download::handle_download_event(&app_for_dl, event)
        })
}

/// 2つの URL が同一オリジン (scheme+host+port) か。どちらか parse 不能なら false。
fn same_origin(a: &str, b: &str) -> bool {
    match (tauri::Url::parse(a), tauri::Url::parse(b)) {
        (Ok(x), Ok(y)) => x.origin() == y.origin(),
        _ => false,
    }
}

fn update_tab_url(app: &AppHandle, bw_label: &str, tab_id: &str, new_url: &str) {
    // Windows 専用: ローカル UI (newtab.html 等) の `http://tauri.localhost/...` を AppState に
    // 入れない (URL バーに表示させない、二重保険)。
    if new_url.starts_with("http://tauri.localhost")
        || new_url.starts_with("https://tauri.localhost")
    {
        return;
    }
    {
        let state = app.state::<AppState>();
        let mut windows = state.windows.write();
        if let Some(bw) = windows.get_mut(bw_label) {
            if let Some(tab) = bw.tabs.iter_mut().find(|t| t.id == tab_id) {
                if tab.url == new_url {
                    return;
                }
                tab.url = new_url.to_string();
            } else {
                eprintln!(
                    "[tab.rs] update_tab_url: tab not found: bw={} tab={}",
                    bw_label, tab_id
                );
                return;
            }
        } else {
            eprintln!("[tab.rs] update_tab_url: bw not found: {}", bw_label);
            return;
        }
    }
    let _ = app.emit_to(
        EventTarget::webview(format!("{}-tabbar", bw_label)),
        "tab://url-changed",
        json!({ "bwLabel": bw_label, "tabId": tab_id, "url": new_url }),
    );
}

fn update_tab_title(app: &AppHandle, bw_label: &str, tab_id: &str, title: &str) {
    let state = app.state::<AppState>();
    let mut windows = state.windows.write();
    if let Some(bw) = windows.get_mut(bw_label) {
        if let Some(tab) = bw.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.title = title.to_string();
        }
    }
}

/// content webview から呼び出される SPA 用の URL 変化通知。注入スクリプト
/// (`inject/url_watch.js`) が pushState/replaceState/popstate/hashchange/title 変化を検知して
/// invoke する。呼出元 webview ラベル `bw_<x>-tab-<y>` から bw/tab を逆算し、AppState を更新。
#[tauri::command]
pub fn report_url_change(
    webview: tauri::Webview,
    app: AppHandle,
    url: String,
    title: Option<String>,
) -> Result<(), String> {
    assert_caller(&webview, &["bw_*-tab-*"])?;
    let label = webview.label().to_string();
    let mut parts = label.rsplitn(2, "-tab-");
    let tab_id = parts
        .next()
        .ok_or_else(|| format!("invalid content label: {}", label))?
        .to_string();
    let bw_label = parts
        .next()
        .ok_or_else(|| format!("invalid content label: {}", label))?
        .to_string();

    // §A.1 セキュリティ: report_url_change は SPA のソフト遷移 (pushState/hashchange) 通知専用で、
    // pushState はブラウザ仕様上 同一オリジンに限られる。よって申告 URL のオリジンが現タブの
    // 信頼済み URL (on_navigation/on_page_load が webview.url() から設定) と異なる場合は、
    // remote ページによるアドレスバー偽装 (フィッシング) とみなして無視する。
    let trusted = {
        let state = app.state::<AppState>();
        let guard = state.windows.read();
        guard
            .get(&bw_label)
            .and_then(|bw| bw.tabs.iter().find(|t| t.id == tab_id))
            .map(|t| t.url.clone())
            .unwrap_or_default()
    };
    if !trusted.is_empty() && !same_origin(&trusted, &url) {
        return Ok(());
    }

    update_tab_url(&app, &bw_label, &tab_id, &url);
    if let Some(t) = title {
        if !t.is_empty() {
            update_tab_title(&app, &bw_label, &tab_id, &t);
            let _ = app.emit_to(
                EventTarget::webview(format!("{}-tabbar", bw_label)),
                "tab://title-changed",
                json!({ "bwLabel": bw_label, "tabId": tab_id, "title": t }),
            );
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn new_tab(
    webview: tauri::Webview,
    app: AppHandle,
    bw_label: String,
    url: String,
    activate: bool,
) -> Result<String, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    open_tab_internal(&app, bw_label, url, activate).await
}

/// 新規 tab を作る内部関数。assert_caller 抜きなので、コマンド層 (`new_tab`)、link 層
/// (`report_link_action` / `on_new_window`) 両方から呼ばれる。プロファイルは親 BW から継承。
pub(crate) async fn open_tab_internal(
    app: &AppHandle,
    bw_label: String,
    url: String,
    activate: bool,
) -> Result<String, String> {
    let state = app.state::<AppState>();

    // 親 BW の profile_id を取得 (継承)
    let profile_id = {
        let guard = state.windows.read();
        guard
            .get(&bw_label)
            .map(|bw| bw.profile_id.clone())
            .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string())
    };

    let window = app
        .get_window(&bw_label)
        .ok_or_else(|| format!("BW not found: {}", bw_label))?;

    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let inner = window.inner_size().map_err(|e| e.to_string())?;
    let width = inner.width as f64 / scale;
    let content_height = ((inner.height as f64 / scale) - TABBAR_HEIGHT).max(0.0);

    let tab_id = state.alloc_tab_id();
    let tab_webview_label = format!("{}-tab-{}", bw_label, tab_id);

    // activate なら現 active タブを画面外へ
    if activate {
        let prev_active_label: Option<String> = {
            let guard = state.windows.read();
            guard.get(&bw_label).and_then(|bw| {
                bw.active_tab_id.as_ref().and_then(|id| {
                    bw.tabs
                        .iter()
                        .find(|t| &t.id == id)
                        .map(|t| t.webview_label.clone())
                })
            })
        };
        if let Some(label) = prev_active_label {
            if let Some(wv) = app.get_webview(&label) {
                let _ = wv.set_position(LogicalPosition::new(0.0, OFFSCREEN_Y));
            }
        }
    }

    let pos_y = if activate { TABBAR_HEIGHT } else { OFFSCREEN_Y };

    // url が空文字なら newtab.html、それ以外は外部 URL。
    let is_newtab = url.is_empty();
    let (state_url, webview_url) = if is_newtab {
        (String::new(), WebviewUrl::App("newtab.html".into()))
    } else {
        (
            url.clone(),
            WebviewUrl::External(url.parse().map_err(|_| format!("invalid URL: {}", url))?),
        )
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
            LogicalPosition::new(0.0, pos_y),
            LogicalSize::new(width, content_height),
        )
        .map_err(|e| e.to_string())?;

    {
        let mut windows = state.windows.write();
        if let Some(bw) = windows.get_mut(&bw_label) {
            bw.tabs.push(Tab {
                id: tab_id.clone(),
                webview_label: tab_webview_label.clone(),
                title: String::new(),
                url: state_url,
            });
            if activate {
                bw.active_tab_id = Some(tab_id.clone());
            }
        }
    }

    let payload = json!({ "bwLabel": bw_label, "tabId": tab_id });
    let _ = app.emit_to(
        EventTarget::webview(format!("{}-tabbar", bw_label)),
        "tab://opened",
        payload.clone(),
    );
    let _ = app.emit_to(EventTarget::webview("console"), "tab://opened", payload);

    Ok(tab_id)
}

#[tauri::command]
pub async fn close_tab(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bw_label: String,
    tab_id: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;

    let (closing_label, was_active, next_active): (
        Option<String>,
        bool,
        Option<(String, String)>,
    ) = {
        let mut windows = state.windows.write();
        let bw = windows
            .get_mut(&bw_label)
            .ok_or_else(|| format!("BW not found: {}", bw_label))?;
        let idx = bw
            .tabs
            .iter()
            .position(|t| t.id == tab_id)
            .ok_or_else(|| format!("tab not found: {}", tab_id))?;
        let closing = bw.tabs.remove(idx);
        let was_active = bw.active_tab_id.as_ref() == Some(&closing.id);

        // 残タブが 0 でなければ「右隣 → 無ければ左隣」を新 active に
        let next_active = if was_active && !bw.tabs.is_empty() {
            let new_idx = if idx < bw.tabs.len() {
                idx
            } else {
                bw.tabs.len() - 1
            };
            let t = &bw.tabs[new_idx];
            Some((t.id.clone(), t.webview_label.clone()))
        } else {
            None
        };

        if was_active {
            bw.active_tab_id = next_active.as_ref().map(|(id, _)| id.clone());
        }

        (Some(closing.webview_label), was_active, next_active)
    };

    if let Some(label) = &closing_label {
        if let Some(wv) = app.get_webview(label) {
            let _ = wv.close();
        }
    }

    let now_empty = {
        let guard = state.windows.read();
        guard
            .get(&bw_label)
            .map(|bw| bw.tabs.is_empty())
            .unwrap_or(true)
    };

    if now_empty {
        // 残タブ 0 → BW ごとクローズ。CloseRequested ハンドラが AppState 整理＋emit を行う。
        let tabbar_label = format!("{}-tabbar", bw_label);
        if let Some(wv) = app.get_webview(&tabbar_label) {
            let _ = wv.close();
        }
        if let Some(window) = app.get_window(&bw_label) {
            let _ = window.close();
        }
        return Ok(());
    }

    if was_active {
        if let Some((new_active_id, new_active_label)) = next_active {
            if let Some(window) = app.get_window(&bw_label) {
                let scale = window.scale_factor().unwrap_or(1.0);
                let inner = window.inner_size().map_err(|e| e.to_string())?;
                let width = inner.width as f64 / scale;
                let content_height = ((inner.height as f64 / scale) - TABBAR_HEIGHT).max(0.0);
                if let Some(wv) = app.get_webview(&new_active_label) {
                    let _ = wv.set_size(LogicalSize::new(width, content_height));
                    let _ = wv.set_position(LogicalPosition::new(0.0, TABBAR_HEIGHT));
                }
            }
            let switched = json!({ "bwLabel": bw_label, "tabId": new_active_id });
            let _ = app.emit_to(
                EventTarget::webview(format!("{}-tabbar", bw_label)),
                "tab://switched",
                switched.clone(),
            );
            let _ = app.emit_to(EventTarget::webview("console"), "tab://switched", switched);
        }
    }

    let closed = json!({ "bwLabel": bw_label, "tabId": tab_id });
    let _ = app.emit_to(
        EventTarget::webview(format!("{}-tabbar", bw_label)),
        "tab://closed",
        closed.clone(),
    );
    let _ = app.emit_to(EventTarget::webview("console"), "tab://closed", closed);

    Ok(())
}

#[tauri::command]
pub fn switch_tab(
    webview: tauri::Webview,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    bw_label: String,
    tab_id: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;

    let window = app
        .get_window(&bw_label)
        .ok_or_else(|| format!("BW not found: {}", bw_label))?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let inner = window.inner_size().map_err(|e| e.to_string())?;
    let width = inner.width as f64 / scale;
    let content_height = ((inner.height as f64 / scale) - TABBAR_HEIGHT).max(0.0);

    let target_label: Option<String> = {
        let mut windows = state.windows.write();
        let bw = windows
            .get_mut(&bw_label)
            .ok_or_else(|| format!("BW not found: {}", bw_label))?;
        if !bw.tabs.iter().any(|t| t.id == tab_id) {
            return Err(format!("tab not found: {}", tab_id));
        }
        bw.active_tab_id = Some(tab_id.clone());
        bw.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.webview_label.clone())
    };

    let all_labels: Vec<String> = {
        let guard = state.windows.read();
        guard
            .get(&bw_label)
            .map(|bw| bw.tabs.iter().map(|t| t.webview_label.clone()).collect())
            .unwrap_or_default()
    };

    for label in &all_labels {
        if let Some(wv) = app.get_webview(label) {
            let _ = wv.set_size(LogicalSize::new(width, content_height));
            if Some(label) == target_label.as_ref() {
                let _ = wv.set_position(LogicalPosition::new(0.0, TABBAR_HEIGHT));
            } else {
                let _ = wv.set_position(LogicalPosition::new(0.0, OFFSCREEN_Y));
            }
        }
    }

    let payload = json!({ "bwLabel": bw_label, "tabId": tab_id });
    let _ = app.emit_to(
        EventTarget::webview(format!("{}-tabbar", bw_label)),
        "tab://switched",
        payload.clone(),
    );
    let _ = app.emit_to(EventTarget::webview("console"), "tab://switched", payload);

    Ok(())
}

#[tauri::command]
pub fn list_tabs(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
    bw_label: String,
) -> Result<Vec<TabSummary>, String> {
    assert_caller(&webview, &["console", "bw_*-tabbar"])?;
    let guard = state.windows.read();
    let bw = guard
        .get(&bw_label)
        .ok_or_else(|| format!("BW not found: {}", bw_label))?;
    let active = bw.active_tab_id.clone();
    Ok(bw
        .tabs
        .iter()
        .map(|t| TabSummary {
            id: t.id.clone(),
            url: t.url.clone(),
            title: t.title.clone(),
            is_active: Some(&t.id) == active.as_ref(),
        })
        .collect())
}
