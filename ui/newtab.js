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
    await invoke('report_link_action', { url, mode: 'self' });
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
