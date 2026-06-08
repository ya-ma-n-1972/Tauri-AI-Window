// newtab は content webview (bw_*-tab-*) として読み込まれるため、__TAURI_INTERNALS__ 経由で invoke する。
const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

function normalizeUrl(input) {
  const u = input.trim();
  if (!u) return null;
  if (!/^[a-z][a-z0-9+.-]*:/i.test(u)) return 'https://' + u;
  return u;
}

async function navigateSelf(url) {
  if (!url) return;
  try {
    // §A.1: newtab は build_content_webview が tauri.localhost ガード付きで注入した nonce を使う。
    const nonce = window.__TAW_NONCE__ || '';
    await invoke('report_link_action', { url, mode: 'self', nonce });
  } catch (err) {
    try { console.warn('[newtab] report_link_action self failed:', err); } catch (_) {}
  }
}

const addr = document.getElementById('addr');
addr.focus();
addr.addEventListener('keydown', async (e) => {
  if (e.key !== 'Enter') return;
  const url = normalizeUrl(addr.value);
  if (!url) return;
  await navigateSelf(url);
});

// ブックマークグリッド。§A.1 セキュア: list_bookmarks は content に公開しないため、
// 生成時に Rust が注入したスナップショット window.__TAW_BOOKMARKS__ を使う。
function renderBookmarks() {
  const grid = document.getElementById('bookmarks');
  const items = Array.isArray(window.__TAW_BOOKMARKS__) ? window.__TAW_BOOKMARKS__ : [];
  grid.innerHTML = '';
  if (!items.length) {
    const d = document.createElement('div');
    d.className = 'empty';
    d.textContent = 'ブックマークはまだありません。タブバーの ☆ で追加できます。';
    grid.appendChild(d);
    return;
  }
  for (const b of items) {
    const card = document.createElement('div');
    card.className = 'card';

    const title = document.createElement('div');
    title.className = 'title';
    title.textContent = b.title || b.url;
    card.appendChild(title);

    const url = document.createElement('div');
    url.className = 'url';
    url.textContent = b.url;
    card.appendChild(url);

    card.addEventListener('click', () => navigateSelf(b.url));
    grid.appendChild(card);
  }
}

renderBookmarks();
