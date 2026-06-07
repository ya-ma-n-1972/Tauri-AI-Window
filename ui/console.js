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

refreshWindows();
