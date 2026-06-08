pub mod bookmark;
pub mod download;
pub mod history;
pub mod link;
pub mod navigation;
pub mod profile;
pub mod tab;
pub mod window;

use tauri::Webview;

/// §A.1: 外部ページ由来 URL を開く経路の許可スキーム。http/https 以外
/// (file:/javascript:/data: 等) を弾く。report_link_action / on_new_window 経路で使う。
pub fn is_http_url(url: &str) -> bool {
    matches!(tauri::Url::parse(url), Ok(u) if matches!(u.scheme(), "http" | "https"))
}

/// §A.1: content webview ごとの nonce を生成する。`RandomState` は OS エントロピーで
/// シードされるため、構築ごとに予測不能な値になる (`rand` 依存なし)。128bit hex。
pub fn gen_nonce(seed: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h1 = std::collections::hash_map::RandomState::new().build_hasher();
    h1.write(seed.as_bytes());
    let a = h1.finish();
    let mut h2 = std::collections::hash_map::RandomState::new().build_hasher();
    h2.write(seed.as_bytes());
    h2.write(&a.to_le_bytes());
    let b = h2.finish();
    format!("{:016x}{:016x}", a, b)
}

// 自作コマンドはどの webview からでも IPC で叩けてしまう前提の二重防御。
// capability 設定ミスがあっても content webview から特権コマンドが通らないように
// IPC 入口でラベルを glob 検証する。
pub fn assert_caller(webview: &Webview, allowed: &[&str]) -> Result<(), String> {
    let label = webview.label();
    for pat in allowed {
        if glob_match(pat, label) {
            return Ok(());
        }
    }
    Err(format!(
        "caller '{}' not allowed (allowed: {:?})",
        label, allowed
    ))
}

// `*` を任意長の文字列にマッチさせる単純グロブ。標準的な two-pointer + backtrack。
fn glob_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut match_idx: usize = 0;

    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn exact() {
        assert!(glob_match("console", "console"));
        assert!(!glob_match("console", "consol"));
    }

    #[test]
    fn star_suffix() {
        assert!(glob_match("bw_*-tabbar", "bw_a-tabbar"));
        assert!(glob_match("bw_*-tabbar", "bw_aa-tabbar"));
        assert!(!glob_match("bw_*-tabbar", "bw_a-tab-x"));
    }

    #[test]
    fn star_double() {
        assert!(glob_match("bw_*-tab-*", "bw_a-tab-bbb"));
        assert!(!glob_match("bw_*-tab-*", "bw_a-tabbar"));
    }
}
