pub mod download;
pub mod link;
pub mod navigation;
pub mod tab;
pub mod window;

use tauri::Webview;

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
