use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

// 上段 navbar (戻る/進む/リロード/アドレスバー) 36px + 下段タブ列 36px の 2 段構成。
pub const TABBAR_HEIGHT: f64 = 72.0;
// 非アクティブ Webview を画面外へ退避させる Y 座標。
// hide() だと WebView2 がタブをアンロードしてセッションが切れるため、可視のまま画面外に置く。
pub const OFFSCREEN_Y: f64 = -100000.0;
// Phase 1 では profile 管理 UI は未実装。全 BW はこの既定プロファイルを使う (Phase 5 で UI 追加)。
pub const DEFAULT_PROFILE_ID: &str = "default";

pub struct AppState {
    pub next_bw_id: AtomicU64,
    pub next_tab_id: AtomicU64,
    pub windows: RwLock<HashMap<String, BrowserWindow>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            next_bw_id: AtomicU64::new(0),
            next_tab_id: AtomicU64::new(0),
            windows: RwLock::new(HashMap::new()),
        }
    }

    pub fn alloc_bw_label(&self) -> String {
        let n = self.next_bw_id.fetch_add(1, Ordering::Relaxed);
        format!("bw_{}", id_to_alpha(n))
    }

    pub fn alloc_tab_id(&self) -> String {
        let n = self.next_tab_id.fetch_add(1, Ordering::Relaxed);
        id_to_alpha(n)
    }
}

pub struct BrowserWindow {
    pub label: String,
    pub tabbar_label: String,
    pub tabs: Vec<Tab>,
    pub active_tab_id: Option<String>,
    pub profile_id: String,
}

pub struct Tab {
    pub id: String,
    pub webview_label: String,
    pub title: String,
    pub url: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWindowSummary {
    pub label: String,
    pub tab_count: usize,
    pub active_tab_id: Option<String>,
    pub profile_id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TabSummary {
    pub id: String,
    pub url: String,
    pub title: String,
    pub is_active: bool,
}

// 双向 base-26: 0 -> "a", 25 -> "z", 26 -> "aa", 27 -> "ab", 51 -> "az", 52 -> "ba", ...
pub fn id_to_alpha(mut n: u64) -> String {
    let mut s = Vec::new();
    loop {
        let r = (n % 26) as u8;
        s.push(b'a' + r);
        n /= 26;
        if n == 0 {
            break;
        }
        n -= 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}
