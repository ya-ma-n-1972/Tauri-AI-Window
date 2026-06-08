// content webview に注入。SPA の URL/タイトル変化を監視し Rust に通知する。
// `on_navigation` / `on_page_load` はハードナビゲーションでしか発火しないため、
// pushState 系の soft navigation を捕捉する目的。
// §A.1: NONCE はクロージャ引数 (top-level に置かない)。report_url_change はこの nonce を要求する。
(function (NONCE) {
  if (window.__taw_url_watch__) return;
  window.__taw_url_watch__ = true;
  // top-frame 限定 (iframe 内 URL を URL バーに反映させない)。
  if (window.top !== window) return;
  // newtab.html などローカル UI は URL 監視から除外 (アドレスバーに tauri.localhost を出さない)。
  // Windows 専用前提なので tauri.localhost のみ判定すれば十分。
  if (window.location.host === 'tauri.localhost') return;

  let lastUrl = '';
  let lastTitle = '';

  function notify() {
    try {
      const url = window.location.href;
      const title = document.title || '';
      if (url === lastUrl && title === lastTitle) return;
      lastUrl = url;
      lastTitle = title;
      if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {
        window.__TAURI_INTERNALS__.invoke('report_url_change', { url, title, nonce: NONCE })
          .catch((e) => { try { console.warn('[taw] report_url_change failed:', e); } catch (_) {} });
      }
    } catch (_) {}
  }

  function attachTitleObserver() {
    const t = document.querySelector('title');
    if (t) {
      try { new MutationObserver(notify).observe(t, { childList: true }); } catch (_) {}
    } else {
      // <title> が後から差し込まれるケース
      try {
        const obs = new MutationObserver(() => {
          const found = document.querySelector('title');
          if (found) {
            try { new MutationObserver(notify).observe(found, { childList: true }); } catch (_) {}
            notify();
            obs.disconnect();
          }
        });
        obs.observe(document.documentElement, { childList: true, subtree: true });
      } catch (_) {}
    }
  }

  // History API を早期にモンキーパッチ
  try {
    const _push = history.pushState;
    history.pushState = function () {
      const r = _push.apply(this, arguments);
      notify();
      return r;
    };
    const _replace = history.replaceState;
    history.replaceState = function () {
      const r = _replace.apply(this, arguments);
      notify();
      return r;
    };
  } catch (_) {}

  window.addEventListener('popstate', notify);
  window.addEventListener('hashchange', notify);

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => { notify(); attachTitleObserver(); }, { once: true });
  } else {
    notify();
    attachTitleObserver();
  }
})("__TAW_NONCE__");
