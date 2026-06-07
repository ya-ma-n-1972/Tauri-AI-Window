const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// §A.1: onclick 文字列補間を避け、addEventListener + dataset で配線する (特権 console への注入余地を排除)。

const addrInput = document.getElementById('addr');

async function openBlank() {
  const url = addrInput.value.trim();
  try {
    await invoke('new_browser_window', { initialUrl: url || null });
    addrInput.value = '';
  } catch (err) {
    console.warn('new_browser_window', err);
  }
}

document.getElementById('open-blank').addEventListener('click', () => {
  addrInput.value = '';
  openBlank();
});
document.getElementById('open-url').addEventListener('click', openBlank);
addrInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') openBlank();
});

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
    span.textContent = `${bw.label} (${bw.tabCount} tabs)`;
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

function dirOf(p) {
  if (!p) return '';
  const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'));
  return i >= 0 ? p.slice(0, i) : p;
}

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

refreshWindows();
renderDownloads();
