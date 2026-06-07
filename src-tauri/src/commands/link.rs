use crate::commands::assert_caller;
use crate::state::{AppState, LinkOpenMode, DEFAULT_PROFILE_ID};
use tauri::{AppHandle, Manager};

/// content webview から呼ばれる「リンク開き分け」通知。注入スクリプト
/// (`inject/link_intercept.js`) が click/auxclick で修飾キー判定後に invoke する。
/// 呼出元 webview ラベル `bw_<x>-tab-<y>` から source の bw を逆算して、新規タブ or
/// 新規ウィンドウを開く。newtab.html からは mode='self' で同タブ遷移にも使う。
#[tauri::command]
pub async fn report_link_action(
    webview: tauri::Webview,
    app: AppHandle,
    url: String,
    mode: String,
) -> Result<(), String> {
    assert_caller(&webview, &["bw_*-tab-*"])?;

    let caller_label = webview.label().to_string();
    let mut parts = caller_label.rsplitn(2, "-tab-");
    parts
        .next()
        .ok_or_else(|| format!("invalid content label: {}", caller_label))?;
    let bw_label = parts
        .next()
        .ok_or_else(|| format!("invalid content label: {}", caller_label))?
        .to_string();

    // §2.5: 'auto' (link_intercept.js が修飾キー無しの target=_blank 等で送る) は、
    // BW のリンク開きモード (スイッチ) を参照して tab/window に解決する (優先順位3)。
    // 'tab'/'window' は明示操作 (優先順位1) なのでそのまま通す。
    let effective_mode = if mode == "auto" {
        let state = app.state::<AppState>();
        let guard = state.windows.read();
        let to_window = guard
            .get(&bw_label)
            .map(|bw| bw.link_open_mode == LinkOpenMode::Window)
            .unwrap_or(false);
        if to_window {
            "window"
        } else {
            "tab"
        }
    } else {
        mode.as_str()
    };

    match effective_mode {
        "tab" => {
            crate::commands::tab::open_tab_internal(&app, bw_label, url, true).await?;
            Ok(())
        }
        "window" => {
            // 親 BW の profile_id を継承
            let parent_profile = {
                let state = app.state::<AppState>();
                let guard = state.windows.read();
                guard
                    .get(&bw_label)
                    .map(|bw| bw.profile_id.clone())
                    .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string())
            };
            crate::commands::window::open_browser_window_internal(&app, Some(url), parent_profile)
                .await?;
            Ok(())
        }
        "self" => {
            // 同タブ navigate (newtab.html → 外部 URL の遷移などに使う)。
            let parsed: tauri::Url = url
                .parse()
                .map_err(|_| format!("invalid URL: {}", url))?;
            if let Some(wv) = app.get_webview(&caller_label) {
                wv.navigate(parsed).map_err(|e| e.to_string())?;
            } else {
                return Err(format!("webview not found: {}", caller_label));
            }
            Ok(())
        }
        _ => Err(format!("invalid mode: {}", mode)),
    }
}
