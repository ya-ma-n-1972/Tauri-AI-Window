// content webview に document-start で注入。
// 背景: 本アプリは 1 ウィンドウに tabbar + 複数 content の WebView2 を重ねる多 webview 構成で、
// 非アクティブタブを hide() せず画面外 (OFFSCREEN_Y) へ退避する。この構成だと WebView2 の
// オクルージョン判定により content の document.visibilityState が 'hidden' を返すことがある
// (Tauri #10592 / WebView2Feedback #1094)。
// Google Keep はノート本文を可視状態に応じて遅延描画するため、'hidden' と誤判定されると
// シェル(ヘッダ/検索)だけ描画してノート本文を一切描かない (= 空白)。YouTube 等 可視性に
// 依存せず描画するサイトは影響を受けないため、症状が Google サービス間で割れていた。
//
// 対策: ページに対して「常に可視・常にフォーカスあり」を申告する。本アプリは設計上
// バックグラウンドタブもセッション/ストリーミングを維持する方針 (スコープ §2.1) なので、
// 全 content で visible を名乗ることは仕様と矛盾しない。メモリ削減は MemoryUsageTargetLevel
// 側が担うため、この override はメモリ最適化を阻害しない。
(function () {
  if (window.__taw_visibility_fix__) return;
  window.__taw_visibility_fix__ = true;
  try {
    var visible = function () { return 'visible'; };
    var notHidden = function () { return false; };
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: visible });
    Object.defineProperty(document, 'hidden', { configurable: true, get: notHidden });
    // 一部サイトは webkit 接頭辞版を参照する。
    Object.defineProperty(document, 'webkitVisibilityState', { configurable: true, get: visible });
    Object.defineProperty(document, 'webkitHidden', { configurable: true, get: notHidden });
    // フォーカス依存で描画を止めるサイト対策。
    document.hasFocus = function () { return true; };
  } catch (_) {}
})();
