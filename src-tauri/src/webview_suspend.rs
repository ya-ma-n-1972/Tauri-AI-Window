//! 実験 (branch: exp/trysuspend): 非アクティブ content webview を
//! `IsVisible=false` + `ICoreWebView2_3::TrySuspend` で休止し、アクティブ化時に
//! `Resume` + `IsVisible=true` で復帰する。Edge 風の省リソース挙動を検証するための最小実装。
//!
//! 完全ベストエフォート: 失敗 (ランタイム未対応 / COM 失敗 / webview 不在) は黙って握りつぶし、
//! 呼び出し元のタブ操作は止めない。COM は webview_mem.rs と同じく `with_webview` 経由で
//! UI スレッド上で実行する (WebView2 スレッドモデル / 再入回避)。
//!
//! 注意: TrySuspend は「非表示 (IsVisible=false)」が前提。アクティブ webview には呼ばない。
//! 位置・サイズの再設定は呼び出し側 (tab.rs) が従来どおり行い、ここでは可視性と suspend/resume
//! だけを担当する。

use tauri::AppHandle;

/// 非アクティブ化: 非表示にして TrySuspend (休止) する。
pub fn suspend(app: &AppHandle, webview_label: &str) {
    #[cfg(windows)]
    suspend_windows(app, webview_label);
    #[cfg(not(windows))]
    {
        let _ = (app, webview_label);
    }
}

/// アクティブ化: Resume してから再表示する。位置/サイズは呼び出し側で再設定済みの前提。
pub fn resume(app: &AppHandle, webview_label: &str) {
    #[cfg(windows)]
    resume_windows(app, webview_label);
    #[cfg(not(windows))]
    {
        let _ = (app, webview_label);
    }
}

#[cfg(windows)]
fn suspend_windows(app: &AppHandle, webview_label: &str) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    use webview2_com::TrySuspendCompletedHandler;
    use windows::core::Interface; // .cast::<T>()

    let Some(webview) = app.get_webview(webview_label) else {
        return;
    };
    let _ = webview.with_webview(move |pw| unsafe {
        let controller = pw.controller();
        // TrySuspend は非表示が前提。まず IsVisible=false にする (失敗したら何もしない)。
        if controller.SetIsVisible(false).is_err() {
            return;
        }
        let Ok(core) = controller.CoreWebView2() else {
            return;
        };
        let Ok(core3) = core.cast::<ICoreWebView2_3>() else {
            return; // ランタイムが ICoreWebView2_3 未満なら諦める
        };
        // 完了は待たない (fire-and-forget)。ハンドラは WebView2 が保持する。
        let handler = TrySuspendCompletedHandler::create(Box::new(
            |_result: windows::core::Result<()>, _succeeded: bool| Ok(()),
        ));
        let _ = core3.TrySuspend(&handler);
    });
}

#[cfg(windows)]
fn resume_windows(app: &AppHandle, webview_label: &str) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    use windows::core::Interface;

    let Some(webview) = app.get_webview(webview_label) else {
        return;
    };
    let _ = webview.with_webview(move |pw| unsafe {
        let controller = pw.controller();
        // 先に Resume (休止していなければ no-op 扱い)、その後で可視化する。
        if let Ok(core) = controller.CoreWebView2() {
            if let Ok(core3) = core.cast::<ICoreWebView2_3>() {
                let _ = core3.Resume();
            }
        }
        let _ = controller.SetIsVisible(true);
    });
}
