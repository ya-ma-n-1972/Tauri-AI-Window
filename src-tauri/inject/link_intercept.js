// content webview に注入。リンククリックの修飾キー判定 + 中ボタン + target="_blank" を捕捉し、
// `report_link_action` に通知して Rust 側で新規タブ/新規ウィンドウを開く。
//
// §2.4: 右クリックは WebView2 のネイティブ標準メニューに委ねるため preventDefault しない。
// §A.1: NONCE はクロージャ引数として渡され (top-level に置かない)、ページ JS から読めない。
//       report_link_action はこの nonce を要求するので外部ページの直接 invoke は弾かれる。
//       さらに e.isTrusted を要求し、ページが合成イベントで本スクリプトを悪用するのを防ぐ。
(function (NONCE) {
  if (window.__taw_link_intercept__) return;
  window.__taw_link_intercept__ = true;

  function report(url, mode) {
    if (!url) return;
    try {
      window.__TAURI_INTERNALS__.invoke('report_link_action', { url, mode, nonce: NONCE })
        .catch((e) => { try { console.warn('[taw] report_link_action:', e); } catch (_) {} });
    } catch (_) {}
  }

  function findAnchor(t) {
    while (t && t.nodeType === 1 && t !== document.body) {
      if (t.tagName === 'A' && t.href) return t;
      t = t.parentElement;
    }
    return null;
  }

  // Ctrl/Cmd+Click, Shift+Ctrl+Click, target=_blank の捕捉。
  // §2.5 優先順位: 修飾キーは明示操作 (優先順位1) なので tab/window を直接指定。
  // 修飾キー無しの target=_blank は 'auto' を送り、Rust 側で BW スイッチに従わせる (優先順位3)。
  document.addEventListener('click', (e) => {
    if (!e.isTrusted) return;
    const a = findAnchor(e.target);
    if (!a) return;
    let mode = null;
    if (e.shiftKey && (e.ctrlKey || e.metaKey)) mode = 'window';
    else if (e.ctrlKey || e.metaKey) mode = 'tab';
    else if (a.target === '_blank') mode = 'auto';
    if (!mode) return;
    e.preventDefault();
    e.stopPropagation();
    report(a.href, mode);
  }, true);

  // 中ボタンクリックは modern browser では `click` 発火しないので auxclick で取る。
  document.addEventListener('auxclick', (e) => {
    if (!e.isTrusted) return;
    if (e.button !== 1) return;
    const a = findAnchor(e.target);
    if (!a) return;
    e.preventDefault();
    e.stopPropagation();
    report(a.href, 'tab');
  }, true);

  // 中ボタン mousedown 自体がスクロール (auto-scroll) に化けないよう抑制。
  document.addEventListener('mousedown', (e) => {
    if (e.button !== 1) return;
    const a = findAnchor(e.target);
    if (a) e.preventDefault();
  }, true);
})("__TAW_NONCE__");
