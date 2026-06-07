use crate::commands::assert_caller;
use crate::state::{AppState, DownloadEntry};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::webview::DownloadEvent;
use tauri::{AppHandle, Emitter, EventTarget, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// content webview の `on_download` から呼ばれるダウンロードイベントハンドラ。
///
/// 方式 (§2.2、ユーザー確定): WebView2 `DownloadStarting` (=メインスレッド同期) 内では
/// モーダルダイアログを出せない (再入ハング) ため、いったん隠しステージング領域へ DL を
/// 流しつつ、即・非同期の保存ダイアログを出す。ダイアログ結果と DL 完了が揃った時点で
/// ステージングから最終保存先へ移動 (キャンセルなら破棄) する。
pub fn handle_download_event(app: &AppHandle, event: DownloadEvent<'_>) -> bool {
    match event {
        DownloadEvent::Requested { url, destination } => {
            on_requested(app, url, destination);
            true
        }
        DownloadEvent::Finished { path, success, .. } => {
            if let Some(p) = path {
                on_finished(app, &p, success);
            }
            true
        }
        _ => true,
    }
}

fn emit_console(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let _ = app.emit_to(EventTarget::webview("console"), event, payload);
}

fn on_requested(app: &AppHandle, url: tauri::Url, destination: &mut PathBuf) {
    let state = app.state::<AppState>();
    let id = state.next_download_id.fetch_add(1, Ordering::Relaxed);

    // 既定ファイル名: WebView2 が提案した名前 → URL 最終セグメント → download.bin。
    let suggested = destination
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            url.path_segments()
                .and_then(|mut s| s.next_back().map(|x| x.to_string()))
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "download.bin".to_string());

    // ステージング(一時保存)先。id 前置でファイル名衝突を避ける。
    let staging_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("download-staging");
    let _ = std::fs::create_dir_all(&staging_dir);
    let staging = staging_dir.join(format!("{}-{}", id, suggested));
    *destination = staging.clone();
    let key = staging.to_string_lossy().to_string();

    {
        let mut map = state.downloads.lock();
        map.insert(
            key.clone(),
            DownloadEntry {
                id,
                staging: staging.clone(),
                target: None,
                decided: false,
                finished: None,
                finalized: false,
            },
        );
    }

    emit_console(
        app,
        "download://started",
        json!({ "id": id, "url": url.to_string(), "name": suggested }),
    );

    // 即・非同期の保存ダイアログ。コールバックは (dialog プラグインの実装上) 別スレッドで走るため
    // メインスレッドを塞がない。初期フォルダは Downloads、既定名は suggested。
    let downloads_dir = dirs::download_dir().unwrap_or_else(|| staging_dir.clone());
    let app_cb = app.clone();
    let key_cb = key.clone();
    app.dialog()
        .file()
        .set_directory(downloads_dir)
        .set_file_name(suggested)
        .save_file(move |result| {
            let target = result.and_then(|fp| fp.into_path().ok());
            {
                let state = app_cb.state::<AppState>();
                let mut map = state.downloads.lock();
                if let Some(e) = map.get_mut(&key_cb) {
                    e.target = target;
                    e.decided = true;
                }
            }
            try_finalize(&app_cb, &key_cb);
        });
}

fn on_finished(app: &AppHandle, path: &Path, success: bool) {
    let key = path.to_string_lossy().to_string();
    {
        let state = app.state::<AppState>();
        let mut map = state.downloads.lock();
        match map.get_mut(&key) {
            Some(e) => e.finished = Some(success),
            None => return, // 自前管理外の DL は無視。
        }
    }
    try_finalize(app, &key);
}

enum FinalizeAction {
    Failed { id: u64, staging: PathBuf },
    Cancel { id: u64, staging: PathBuf },
    Move { id: u64, staging: PathBuf, target: PathBuf },
}

/// ダイアログ結果と DL 完了が揃ったら、移動 or 破棄を一度だけ実行する。
/// `Finished` (メインスレッド) と save_file コールバック (別スレッド) の両方から呼ばれ得るため、
/// `finalized` フラグで二重実行を防ぐ。FS 操作と emit はロック外で行う。
fn try_finalize(app: &AppHandle, key: &str) {
    let action: Option<FinalizeAction> = {
        let state = app.state::<AppState>();
        let mut map = state.downloads.lock();
        let Some(e) = map.get_mut(key) else {
            return;
        };
        if e.finalized || !e.decided || e.finished.is_none() {
            return;
        }
        e.finalized = true;
        let success = e.finished.unwrap_or(false);
        let act = if !success {
            FinalizeAction::Failed {
                id: e.id,
                staging: e.staging.clone(),
            }
        } else if let Some(t) = e.target.clone() {
            FinalizeAction::Move {
                id: e.id,
                staging: e.staging.clone(),
                target: t,
            }
        } else {
            FinalizeAction::Cancel {
                id: e.id,
                staging: e.staging.clone(),
            }
        };
        map.remove(key);
        Some(act)
    };

    let Some(act) = action else {
        return;
    };
    match act {
        FinalizeAction::Failed { id, staging } => {
            let _ = std::fs::remove_file(&staging);
            emit_console(
                app,
                "download://finished",
                json!({ "id": id, "success": false, "path": null }),
            );
        }
        FinalizeAction::Cancel { id, staging } => {
            let _ = std::fs::remove_file(&staging);
            emit_console(app, "download://canceled", json!({ "id": id }));
        }
        FinalizeAction::Move {
            id,
            staging,
            target,
        } => match move_file(&staging, &target) {
            Ok(()) => emit_console(
                app,
                "download://finished",
                json!({ "id": id, "success": true, "path": target.to_string_lossy() }),
            ),
            Err(e) => {
                eprintln!("[download] move failed: {}", e);
                let _ = std::fs::remove_file(&staging);
                emit_console(
                    app,
                    "download://finished",
                    json!({ "id": id, "success": false, "path": null }),
                );
            }
        },
    }
}

/// ステージング → 最終保存先へ移動。まず rename、ドライブ跨ぎ等で失敗したら copy + remove。
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)?;
            Ok(())
        }
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
