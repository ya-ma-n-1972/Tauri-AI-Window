use crate::commands::assert_caller;
use crate::state::{AppState, DownloadEntry};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::webview::DownloadEvent;
use tauri::{AppHandle, Emitter, EventTarget, Manager};
use tauri_plugin_opener::OpenerExt;

/// content webview の `on_download` から呼ばれるダウンロードイベントハンドラ。
///
/// 方式 (§2.2 改訂、2026-06-08): wry/WebView2 の `DownloadStarting` はメインスレッド同期実行で、
/// ここで保存ダイアログを出すと再入ハングする。よって保存ダイアログは出さず、アプリ管理フォルダ
/// (`app_local_data_dir/downloads/`) へ自動保存し、完了時にフォルダを開いてユーザーに任意移動を促す。
pub fn handle_download_event(app: &AppHandle, event: DownloadEvent<'_>) -> bool {
    match event {
        DownloadEvent::Requested { url, destination } => {
            on_requested(app, url, destination);
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            on_finished(app, url.as_str(), path.as_deref(), success);
            true
        }
        _ => true,
    }
}

fn emit_console(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let _ = app.emit_to(EventTarget::webview("console"), event, payload);
}

/// `dir/filename` が既存なら `name (1).ext` のように連番で空きパスを返す (無言上書き回避)。
fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let first = dir.join(filename);
    if !first.exists() {
        return first;
    }
    let p = Path::new(filename);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| filename.to_string());
    let ext = p.extension().map(|s| s.to_string_lossy().to_string());
    let mut i = 1u32;
    loop {
        let name = match &ext {
            Some(e) => format!("{} ({}).{}", stem, i, e),
            None => format!("{} ({})", stem, i),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

fn on_requested(app: &AppHandle, url: tauri::Url, destination: &mut PathBuf) {
    let state = app.state::<AppState>();
    let id = state.next_download_id.fetch_add(1, Ordering::Relaxed);

    // 既定ファイル名: WebView2 提案名 → URL 最終セグメント → download.bin。
    let filename = destination
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            url.path_segments()
                .and_then(|mut s| s.next_back().map(|x| x.to_string()))
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "download.bin".to_string());

    // アプリ管理フォルダ app_local_data_dir/downloads/ に保存。
    let dl_dir = app
        .path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("downloads");
    let _ = std::fs::create_dir_all(&dl_dir);
    let final_path = unique_path(&dl_dir, &filename);
    *destination = final_path.clone();

    let key = final_path.to_string_lossy().to_string();
    {
        let mut map = state.downloads.lock();
        map.insert(
            key,
            DownloadEntry {
                id,
                url: url.to_string(),
                path: final_path.clone(),
            },
        );
    }

    emit_console(
        app,
        "download://started",
        json!({ "id": id, "name": filename, "path": final_path.to_string_lossy() }),
    );
}

fn on_finished(app: &AppHandle, url: &str, path: Option<&Path>, success: bool) {
    let entry = {
        let state = app.state::<AppState>();
        let mut map = state.downloads.lock();
        // 成功時は最終保存パスがキー。失敗(path=None)時は url で進行中エントリを対応付ける。
        let key = match path {
            Some(p) => p.to_string_lossy().to_string(),
            None => map
                .iter()
                .find(|(_, e)| e.url == url)
                .map(|(k, _)| k.clone())
                .unwrap_or_default(),
        };
        map.remove(&key)
    };
    let Some(e) = entry else {
        return; // 自前管理外 / 既に処理済み。
    };

    if success {
        emit_console(
            app,
            "download://finished",
            json!({ "id": e.id, "success": true, "path": e.path.to_string_lossy() }),
        );
        // 完了したら保存フォルダを開き、可能なら対象ファイルを選択表示する。
        let _ = app.opener().reveal_item_in_dir(&e.path);
    } else {
        // 失敗時はゴミファイルが残り得るので best-effort で削除。
        let _ = std::fs::remove_file(&e.path);
        emit_console(
            app,
            "download://finished",
            json!({ "id": e.id, "success": false, "path": e.path.to_string_lossy() }),
        );
    }
}

/// 完了したダウンロードを既定アプリで開く。
#[tauri::command]
pub fn open_download_file(
    webview: tauri::Webview,
    app: AppHandle,
    path: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console"])?;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// 完了したダウンロードをエクスプローラで表示 (フォルダを開いて選択状態にする)。
#[tauri::command]
pub fn open_download_folder(
    webview: tauri::Webview,
    app: AppHandle,
    path: String,
) -> Result<(), String> {
    assert_caller(&webview, &["console"])?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}
