// content webview に注入する JS をビルド時に文字列定数として埋め込む。
pub const URL_WATCH_JS: &str = include_str!("../inject/url_watch.js");
pub const LINK_INTERCEPT_JS: &str = include_str!("../inject/link_intercept.js");
