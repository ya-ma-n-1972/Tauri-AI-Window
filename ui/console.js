const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// §A.1: onclick 文字列補間を避け、addEventListener + dataset で配線する (特権 console への注入余地を排除)。

const addrInput = document.getElementById('addr');

let profilesCache = []; // [{ id, name, createdAt }]
function profileName(id) {
  const p = profilesCache.find(x => x.id === id);
  return p ? p.name : id;
}

function selectedProfile() {
  const sel = document.getElementById('profile');
  return sel.value || 'default';
}

async function openBlank() {
  const url = addrInput.value.trim();
  try {
    await invoke('new_browser_window', { initialUrl: url || null, profileId: selectedProfile() });
    addrInput.value = '';
  } catch (err) {
    console.warn('new_browser_window', err);
  }
}

async function openUrlInWindow(url) {
  try {
    await invoke('new_browser_window', { initialUrl: url, profileId: selectedProfile() });
  } catch (err) {
    console.warn('new_browser_window', err);
  }
}

function fmtTs(t) {
  try { return new Date((t || 0) * 1000).toLocaleString(); } catch (_) { return ''; }
}

document.getElementById('open-blank').addEventListener('click', () => {
  addrInput.value = '';
  openBlank();
});
document.getElementById('open-url').addEventListener('click', openBlank);
addrInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') openBlank();
});

// AI サイトの直リンボタン (選択プロファイルで新規ウィンドウ)。サイトを増減するにはこの配列を編集。
const AI_SITES = [
  { label: 'ChatGPT', url: 'https://chat.openai.com/' },
  { label: 'Claude', url: 'https://claude.ai/' },
  { label: 'Gemini', url: 'https://gemini.google.com/' },
];

function renderAiSites() {
  const row = document.getElementById('ai-sites');
  for (const s of AI_SITES) {
    const btn = document.createElement('button');
    btn.textContent = s.label;
    btn.dataset.url = s.url;
    btn.addEventListener('click', () => openUrlInWindow(btn.dataset.url));
    row.appendChild(btn);
  }
}
renderAiSites();

async function refreshWindows() {
  let list = [];
  try {
    list = await invoke('list_browser_windows');
  } catch (err) {
    console.warn('list_browser_windows', err);
  }
  const ul = document.getElementById('bw-list');
  ul.innerHTML = '';
  if (!list.length) {
    const li = document.createElement('li');
    li.className = 'empty';
    li.textContent = '(なし)';
    ul.appendChild(li);
    return;
  }
  for (const bw of list) {
    const li = document.createElement('li');

    const span = document.createElement('span');
    span.className = 'grow';
    span.appendChild(document.createTextNode(`${bw.label} (${bw.tabCount} tabs)`));
    const sub = document.createElement('span');
    sub.className = 'sub';
    sub.textContent = ` — ${profileName(bw.profileId)}`;
    span.appendChild(sub);
    li.appendChild(span);

    const focusBtn = document.createElement('button');
    focusBtn.textContent = 'フォーカス';
    focusBtn.dataset.label = bw.label;
    focusBtn.addEventListener('click', () => {
      invoke('focus_browser_window', { bwLabel: focusBtn.dataset.label }).catch(e => console.warn('focus', e));
    });
    li.appendChild(focusBtn);

    const closeBtn = document.createElement('button');
    closeBtn.textContent = '閉じる';
    closeBtn.dataset.label = bw.label;
    closeBtn.addEventListener('click', () => {
      invoke('close_browser_window', { bwLabel: closeBtn.dataset.label }).catch(e => console.warn('close', e));
    });
    li.appendChild(closeBtn);

    ul.appendChild(li);
  }
}

listen('bw://opened', refreshWindows);
listen('bw://closed', refreshWindows);
listen('tab://opened', refreshWindows);
listen('tab://closed', refreshWindows);

// === §2.2 ダウンロード一覧 ===
// downloads: [{ id, name, url, status: 'downloading'|'done'|'failed'|'canceled', path }]
const downloads = [];

function renderDownloads() {
  const ul = document.getElementById('downloads');
  ul.innerHTML = '';
  if (!downloads.length) {
    const li = document.createElement('li');
    li.className = 'empty';
    li.textContent = '(なし)';
    ul.appendChild(li);
    return;
  }
  // 新しいものを上に。
  for (const d of downloads.slice().reverse()) {
    const li = document.createElement('li');
    if (d.status === 'failed') li.className = 'dl-failed';
    else if (d.status === 'canceled') li.className = 'dl-canceled';

    const span = document.createElement('span');
    span.className = 'grow';
    const sub = d.status === 'done' ? (d.path || '') : d.url;
    span.title = sub;
    span.innerHTML = '';
    span.appendChild(document.createTextNode(d.name || '(no name)'));
    const br = document.createElement('br');
    span.appendChild(br);
    const subSpan = document.createElement('span');
    subSpan.className = 'sub';
    subSpan.textContent = sub;
    span.appendChild(subSpan);
    li.appendChild(span);

    const status = document.createElement('span');
    status.className = 'status';
    status.textContent = ({
      downloading: 'ダウンロード中／保存先選択待ち',
      done: '完了',
      failed: '失敗',
      canceled: 'キャンセル',
    })[d.status] || d.status;
    li.appendChild(status);

    if (d.status === 'done' && d.path) {
      const openBtn = document.createElement('button');
      openBtn.textContent = 'ファイルを開く';
      openBtn.dataset.path = d.path;
      openBtn.addEventListener('click', () => {
        invoke('open_download_file', { path: openBtn.dataset.path }).catch(e => console.warn('open_download_file', e));
      });
      li.appendChild(openBtn);

      const folderBtn = document.createElement('button');
      folderBtn.textContent = 'フォルダを開く';
      folderBtn.dataset.path = d.path;
      folderBtn.addEventListener('click', () => {
        invoke('open_download_folder', { path: folderBtn.dataset.path }).catch(e => console.warn('open_download_folder', e));
      });
      li.appendChild(folderBtn);
    }

    ul.appendChild(li);
  }
}

listen('download://started', e => {
  downloads.push({ id: e.payload.id, name: e.payload.name, url: e.payload.url, status: 'downloading', path: null });
  renderDownloads();
});
listen('download://finished', e => {
  const d = downloads.find(x => x.id === e.payload.id);
  if (!d) return;
  if (e.payload.success) { d.status = 'done'; d.path = e.payload.path; }
  else { d.status = 'failed'; }
  renderDownloads();
});
listen('download://canceled', e => {
  const d = downloads.find(x => x.id === e.payload.id);
  if (!d) return;
  d.status = 'canceled';
  renderDownloads();
});

// === 共通: リスト項目を生成 (title 行 + sub 行 + 任意ボタン) ===
function makeItem(title, sub, buttons, extraClass) {
  const li = document.createElement('li');
  if (extraClass) li.className = extraClass;
  const span = document.createElement('span');
  span.className = 'grow';
  span.title = sub || '';
  span.appendChild(document.createTextNode(title || '(no title)'));
  if (sub) {
    span.appendChild(document.createElement('br'));
    const s = document.createElement('span');
    s.className = 'sub';
    s.textContent = sub;
    span.appendChild(s);
  }
  li.appendChild(span);
  for (const b of (buttons || [])) {
    const btn = document.createElement('button');
    btn.textContent = b.label;
    btn.addEventListener('click', b.onClick);
    li.appendChild(btn);
  }
  return li;
}

function fillList(ulId, items, emptyText) {
  const ul = document.getElementById(ulId);
  ul.innerHTML = '';
  if (!items.length) {
    const li = document.createElement('li');
    li.className = 'empty';
    li.textContent = emptyText;
    ul.appendChild(li);
    return false;
  }
  for (const it of items) ul.appendChild(it);
  return true;
}

// === ブックマーク ===
async function refreshBookmarks() {
  let items = [];
  try { items = await invoke('list_bookmarks'); } catch (err) { console.warn('list_bookmarks', err); }
  fillList('bookmarks', items.map(b => makeItem(
    b.title || b.url, b.url,
    [
      { label: '開く', onClick: () => openUrlInWindow(b.url) },
      { label: '削除', onClick: async () => { try { await invoke('remove_bookmark', { url: b.url }); await refreshBookmarks(); } catch (e) { console.warn('remove_bookmark', e); } } },
    ]
  )), '(まだありません)');
}

// === 履歴 ===
async function refreshHistory() {
  let items = [];
  try { items = await invoke('list_history'); } catch (err) { console.warn('list_history', err); }
  fillList('history', items.slice(0, 100).map(h => makeItem(
    h.title || h.url, `${h.url} — ${fmtTs(h.visitedAt)}`,
    [{ label: '開く', onClick: () => openUrlInWindow(h.url) }]
  )), '(まだありません)');
}

document.getElementById('clear-history').addEventListener('click', async () => {
  try { await invoke('clear_history'); await refreshHistory(); } catch (e) { console.warn('clear_history', e); }
});

// === プロファイル ===
async function refreshProfiles() {
  let items = [];
  try { items = await invoke('list_profiles'); } catch (err) { console.warn('list_profiles', err); }
  profilesCache = items;

  // ドロップダウン (現在選択を保持)
  const sel = document.getElementById('profile');
  const prev = sel.value;
  sel.innerHTML = '';
  for (const p of items) {
    const opt = document.createElement('option');
    opt.value = p.id;
    opt.textContent = p.name;
    sel.appendChild(opt);
  }
  if (items.some(p => p.id === prev)) sel.value = prev;

  // 管理リスト
  fillList('profile-list', items.map(p => {
    const removable = p.id !== 'default';
    const buttons = removable
      ? [{ label: '削除', onClick: async () => {
          if (!confirm('このプロファイルを削除しますか？(使用中のウィンドウがあると失敗します)')) return;
          try { await invoke('remove_profile', { id: p.id }); await refreshProfiles(); }
          catch (e) { alert('削除エラー: ' + e); }
        } }]
      : [];
    return makeItem(p.name, `id: ${p.id}` + (removable ? '' : '（既定）'), buttons);
  }), '(なし)');

  // ウィンドウ一覧のプロファイル名表示を更新。
  await refreshWindows();
}

async function addProfile() {
  const input = document.getElementById('profile-name');
  const name = input.value.trim();
  if (!name) return;
  try { await invoke('add_profile', { name }); input.value = ''; await refreshProfiles(); }
  catch (e) { alert('プロファイル追加エラー: ' + e); }
}
document.getElementById('add-profile').addEventListener('click', addProfile);
document.getElementById('profile-name').addEventListener('keydown', e => { if (e.key === 'Enter') addProfile(); });

// Rust 側の変更通知で再取得（tabbar ☆ での追加・ページ閲覧での履歴記録を即反映）。
listen('bookmark://changed', refreshBookmarks);
listen('history://changed', refreshHistory);

refreshWindows();
renderDownloads();
refreshProfiles();
refreshBookmarks();
refreshHistory();
