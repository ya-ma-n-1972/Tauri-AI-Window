//! §2.1: content webview の WebView2 MemoryUsageTargetLevel を Low/Normal に切り替える。
//! Tauri は with_webview で ICoreWebView2Controller しか渡さない (wry の WebView は非公開) ため、
//! wry 0.55.1 の set_memory_usage_level と同じ COM 呼び出し
//! (controller().CoreWebView2() -> cast::<ICoreWebView2_19>() -> SetMemoryUsageTargetLevel)
//! を自前で行う。完全ベストエフォート: 失敗は握りつぶし、呼び出し元の機能は止めない。

use tauri::AppHandle;

/// `webview_label` の content webview のメモリ目標レベルを設定する。
/// `low=true` で Low(=1)、`low=false` で Normal(=0)。
/// webview が存在しない・取得失敗・COM 失敗のいずれでも黙って何もしない。
pub fn set_memory_level(app: &AppHandle, webview_label: &str, low: bool) {
    #[cfg(windows)]
    set_memory_level_windows(app, webview_label, low);
    #[cfg(not(windows))]
    {
        let _ = (app, webview_label, low); // 非 Windows は no-op
    }
}

#[cfg(windows)]
fn set_memory_level_windows(app: &AppHandle, webview_label: &str, low: bool) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
    };
    use windows::core::Interface; // .cast::<T>() を提供

    let Some(webview) = app.get_webview(webview_label) else {
        return;
    };
    if low && is_google_workspace_like(&webview) {
        return;
    }
    // with_webview は main thread では即時実行され得る。内部は短い COM 呼び出しのみで、
    // webview の生成/クローズ・再入を一切行わないため Windows デッドロック規約 (§1-3) を満たす。
    let _ = webview.with_webview(move |pw| unsafe {
        let controller = pw.controller();
        let Ok(core) = controller.CoreWebView2() else {
            return;
        };
        let Ok(core19) = core.cast::<ICoreWebView2_19>() else {
            return; // ランタイムが ICoreWebView2_19 未満なら静かに諦める
        };
        let level = COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL(if low { 1 } else { 0 });
        let _ = core19.SetMemoryUsageTargetLevel(level);
    });
}

#[cfg(windows)]
fn is_google_workspace_like(webview: &tauri::Webview) -> bool {
    let Ok(url) = webview.url() else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };

    matches!(
        host,
        "accounts.google.com"
            | "calendar.google.com"
            | "chat.google.com"
            | "contacts.google.com"
            | "docs.google.com"
            | "drive.google.com"
            | "keep.google.com"
            | "mail.google.com"
            | "meet.google.com"
            | "photos.google.com"
            | "tasks.google.com"
    )
}
