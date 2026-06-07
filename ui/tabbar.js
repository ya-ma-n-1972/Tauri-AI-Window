const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWebview } = window.__TAURI__.webview;

// 自分のラベル "bw_<id>-tabbar" から bw_label を逆算
const myLabel = getCurrentWebview().label;
const BW = myLabel.replace(/-tabbar$/, '');

// active タブをローカルにキャッシュ (refresh のたびに list_tabs から取得)
let activeTabId = null;
let loadingTabs = new Set();
// スピナの最低表示時間 (ms): 表示開始から 300ms は最低でも見せる。
const SPINNER_MIN_MS = 300;
let spinnerShownAt = new Map(); // tabId -> Date.now() when shown
let spinnerHideTimer = new Map(); // tabId -> timer id

async function switchTab(id) { await invoke('switch_tab', { bwLabel: BW, tabId: id }); }
async function closeTabBtn(id) { await invoke('close_tab', { bwLabel: BW, tabId: id }); }

document.getElementById('add').addEventListener('click', () => {
  invoke('new_tab', { bwLabel: BW, url: '', activate: true }).catch(e => console.warn('new_tab', e));
});
document.getElementById('back').addEventListener('click', () => {
  if (activeTabId) invoke('go_back', { bwLabel: BW, tabId: activeTabId }).catch(e => console.warn('go_back', e));
});
document.getElementById('forward').addEventListener('click', () => {
  if (activeTabId) invoke('go_forward', { bwLabel: BW, tabId: activeTabId }).catch(e => console.warn('go_forward', e));
});
document.getElementById('reload').addEventListener('click', () => {
  if (activeTabId) invoke('reload_tab', { bwLabel: BW, tabId: activeTabId }).catch(e => console.warn('reload', e));
});

// §2.5: per-window リンク開きモードのトグル (非永続)。起動時に Rust の状態で初期化。
let linkMode = 'tab';
const linkModeBtn = document.getElementById('linkmode');

function renderLinkMode() {
  linkModeBtn.textContent = linkMode === 'window' ? 'リンク: ウィンドウ' : 'リンク: タブ';
  linkModeBtn.classList.toggle('window', linkMode === 'window');
}

async function initLinkMode() {
  try {
    linkMode = await invoke('get_link_open_mode', { bwLabel: BW });
  } catch (err) {
    console.warn('get_link_open_mode', err);
    linkMode = 'tab';
  }
  renderLinkMode();
}

linkModeBtn.addEventListener('click', async () => {
  const next = linkMode === 'window' ? 'tab' : 'window';
  try {
    await invoke('set_link_open_mode', { bwLabel: BW, mode: next });
    linkMode = next;
    renderLinkMode();
  } catch (err) {
    console.warn('set_link_open_mode', err);
  }
});

// === ブックマークトグル (§Phase 5) ===
const bookmarkBtn = document.getElementById('bookmark-btn');
let currentUrl = '';

async function updateBookmarkButton() {
  if (!currentUrl) {
    bookmarkBtn.textContent = '☆';
    bookmarkBtn.classList.remove('on');
    return;
  }
  try {
    const on = await invoke('is_bookmarked', { url: currentUrl });
    bookmarkBtn.textContent = on ? '★' : '☆';
    bookmarkBtn.classList.toggle('on', on);
  } catch (_) {}
}

bookmarkBtn.addEventListener('click', async () => {
  if (!currentUrl) return;
  try {
    const on = await invoke('is_bookmarked', { url: currentUrl });
    if (on) {
      await invoke('remove_bookmark', { url: currentUrl });
    } else {
      const tabs = await invoke('list_tabs', { bwLabel: BW });
      const t = tabs.find(x => x.id === activeTabId);
      await invoke('add_bookmark', { url: currentUrl, title: (t && t.title) || currentUrl });
    }
    await updateBookmarkButton();
  } catch (err) {
    console.warn('bookmark toggle', err);
  }
});

const addrInput = document.getElementById('addr');
addrInput.addEventListener('keydown', async (e) => {
  if (e.key === 'Enter' && activeTabId) {
    const url = addrInput.value.trim();
    if (!url) return;
    try {
      await invoke('navigate_tab', { bwLabel: BW, tabId: activeTabId, url });
    } catch (err) {
      console.warn('navigate_tab error', err);
    }
  }
});

function updateSpinner() {
  const el = document.getElementById('spinner');
  const isLoading = !!(activeTabId && loadingTabs.has(activeTabId));
  el.classList.toggle('active', isLoading);
}

function showSpinner(tabId) {
  loadingTabs.add(tabId);
  if (!spinnerShownAt.has(tabId)) spinnerShownAt.set(tabId, Date.now());
  const t = spinnerHideTimer.get(tabId);
  if (t) { clearTimeout(t); spinnerHideTimer.delete(tabId); }
  updateSpinner();
}

function hideSpinner(tabId, fixedDelay) {
  // fixedDelay が指定されていればそれを優先 (SPA url-changed の 700ms 表示等)。
  // 無ければ最低表示時間 SPINNER_MIN_MS を保証する。
  const since = spinnerShownAt.get(tabId) || Date.now();
  const elapsed = Date.now() - since;
  const remaining = fixedDelay !== undefined ? fixedDelay : Math.max(0, SPINNER_MIN_MS - elapsed);
  const prev = spinnerHideTimer.get(tabId);
  if (prev) clearTimeout(prev);
  const timer = setTimeout(() => {
    loadingTabs.delete(tabId);
    spinnerShownAt.delete(tabId);
    spinnerHideTimer.delete(tabId);
    updateSpinner();
  }, remaining);
  spinnerHideTimer.set(tabId, timer);
}

// §A.1: onclick 文字列補間を避け、要素生成 + dataset + addEventListener で配線する。
function makeTabButton(t) {
  const btn = document.createElement('button');
  if (t.isActive) btn.classList.add('active');
  btn.dataset.id = t.id;
  btn.title = t.url || '';

  const label = document.createElement('span');
  label.className = 'label';
  label.textContent = t.title || t.url || '(loading)';
  btn.appendChild(label);

  const close = document.createElement('span');
  close.className = 'close';
  close.textContent = '×';
  close.addEventListener('click', (e) => {
    e.stopPropagation();
    closeTabBtn(t.id);
  });
  btn.appendChild(close);

  btn.addEventListener('click', () => switchTab(t.id));
  return btn;
}

async function refresh() {
  const tabs = await invoke('list_tabs', { bwLabel: BW });
  const div = document.getElementById('tabs');
  const active = tabs.find(t => t.isActive);
  activeTabId = active ? active.id : null;
  currentUrl = active ? (active.url || '') : '';
  if (active) {
    if (document.activeElement !== addrInput) {
      addrInput.value = active.url;
    }
  } else {
    addrInput.value = '';
  }
  div.innerHTML = '';
  for (const t of tabs) div.appendChild(makeTabButton(t));
  updateSpinner();
  updateBookmarkButton();
}

listen('tab://opened', e => { if (e.payload.bwLabel === BW) refresh(); });
listen('tab://closed', e => {
  if (e.payload.bwLabel === BW) {
    loadingTabs.delete(e.payload.tabId);
    refresh();
  }
});
listen('tab://switched', e => { if (e.payload.bwLabel === BW) refresh(); });
listen('tab://title-changed', e => { if (e.payload.bwLabel === BW) refresh(); });
listen('tab://url-changed', e => {
  if (e.payload.bwLabel !== BW) return;
  if (e.payload.tabId === activeTabId) {
    currentUrl = e.payload.url || '';
    updateBookmarkButton();
    if (document.activeElement !== addrInput) {
      addrInput.value = e.payload.url;
    }
  }
  // SPA は load 概念がないので URL 変化時に固定 700ms スピナ。
  showSpinner(e.payload.tabId);
  hideSpinner(e.payload.tabId, 700);
  refresh();
});
listen('tab://load-started', e => {
  if (e.payload.bwLabel !== BW) return;
  showSpinner(e.payload.tabId);
});
listen('tab://load-finished', e => {
  if (e.payload.bwLabel !== BW) return;
  hideSpinner(e.payload.tabId);
});

refresh();
initLinkMode();
