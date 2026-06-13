// content webview に注入する JS をビルド時に文字列定数として埋め込む。
pub const URL_WATCH_JS: &str = include_str!("../inject/url_watch.js");
pub const LINK_INTERCEPT_JS: &str = include_str!("../inject/link_intercept.js");
// WebView2 多 webview のオクルージョン誤判定で document.visibilityState が 'hidden' になり、
// Keep 等が本文を描画しなくなる問題への対策 (常に visible を申告)。詳細は visibility_fix.js。
pub const VISIBILITY_FIX_JS: &str = include_str!("../inject/visibility_fix.js");
