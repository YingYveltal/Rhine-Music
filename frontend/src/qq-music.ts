import { nativeInvoke } from './native';
import './qq-music.css';
export interface QqStatus {
  connected: boolean; officialConfigured: boolean; enabled: boolean; remembered?: boolean;
  playlistCount: number; trackCount: number; updatedAt: string;
  job: { running: boolean; completed?: number; total?: number; message?: string; error?: string };
}
const call = <T = QqStatus>(operation: string, body: object = {}) =>
  nativeInvoke<T>('qq_request', { operation, body });
export function mountQqPanel(root: HTMLElement, refresh: () => Promise<void>, reveal: (id: string) => void) {
  root.innerHTML = `<h3>QQ 音乐</h3><p id="qq-status" role="status">正在读取连接状态…</p>
    <div class="panel-actions"><button class="primary-button" id="qq-sync">同步我的曲库 ↗</button><button id="qq-login">QQ 扫码登录</button><button id="qq-local">读取本机 QQ 曲库</button></div>
    <div id="qq-qr" hidden><img alt="QQ 音乐登录二维码" width="180" height="180"><p role="status"></p></div>
    <label class="settings-row"><span>在专辑架显示 QQ 音乐</span><input id="qq-enabled" type="checkbox"></label>
    <form id="qq-search-form" class="qq-search"><input type="search" id="qq-query" aria-label="在线搜索 QQ 音乐" placeholder="在线搜索歌曲、歌手" required maxlength="150"><button>搜索 ↗</button></form>
    <button id="qq-daily" class="text-button">打开每日推荐 ↗</button>
    <details><summary>连接设置</summary><p>扫码用于个人曲库与账户允许的播放；官方 API Key 用于搜索和每日推荐。可选择保存在本机钥匙串，下次打开自动连接。</p>
    <form id="qq-key-form"><input id="qq-key" type="password" autocomplete="off" aria-label="QQ 音乐官方 API Key" placeholder="输入官方 API Key"><button class="text-button">验证并使用 Key ↗</button></form>
    <div class="panel-actions"><button id="qq-remember">保存连接到钥匙串</button><button id="qq-logout">断开并忘记连接</button></div></details>
    <p id="qq-feedback" role="status" aria-live="polite"></p>`;
  let busy = false, pollingQr = false;
  const el = <T extends HTMLElement = HTMLElement>(id: string) => root.querySelector<T>(`#qq-${id}`)!;
  const feedback = (text: string) => { el('feedback').textContent = text; };
  const show = (s: QqStatus) => {
    if (!root.isConnected) return;
    el('status').textContent = `${s.connected ? '账户已连接' : '账户未连接'} · ${s.officialConfigured ? '官方 Key 已配置' : '官方 Key 未配置'}${s.remembered ? ' · 已记住连接' : ''}\n${s.playlistCount} 个歌单 / 专辑 · ${s.trackCount} 个曲目条目${s.job.running ? ` · ${s.job.message} (${s.job.completed || 0}/${s.job.total || '…'})` : s.job.error ? `\n${s.job.error}` : s.updatedAt ? `\n上次同步 ${new Date(s.updatedAt).toLocaleString('zh-CN')}` : ''}`;
    el<HTMLInputElement>('enabled').checked = s.enabled;
    el<HTMLButtonElement>('sync').disabled = busy || s.job.running || !s.connected;
    el<HTMLButtonElement>('login').disabled = busy || s.job.running;
  };
  const run = async (work: () => Promise<void>) => {
    if (busy) return; busy = true;
    root.setAttribute('aria-busy', 'true'); feedback('正在处理…');
    try { await work(); } catch (e) { feedback((e as Error).message); }
    finally { busy = false; root.removeAttribute('aria-busy'); if (root.isConnected) void call('status').then(show).catch(() => {}); }
  };
  const update = async () => {
    if (!root.isConnected) return;
    try { const s = await call('status'); show(s); } catch (e) { feedback((e as Error).message); }
    if (root.isConnected) setTimeout(() => void update(), 1500);
  };
  const checkQr = async () => {
    if (!root.isConnected || !pollingQr) return;
    try {
      const result = await call<{ connected?: boolean; expired?: boolean; message: string }>('poll');
      el('qr').querySelector('p')!.textContent = result.message;
      if (result.connected) {
        pollingQr = false; el('qr').hidden = true; await call('sync'); await refresh();
        feedback('登录成功，正在同步歌单与收藏专辑。'); return;
      }
      if (result.expired) { pollingQr = false; return; }
    } catch (e) { feedback((e as Error).message); pollingQr = false; return; }
    if (root.isConnected) setTimeout(() => void checkQr(), 2000);
  };
  el('login').onclick = () => void run(async () => {
    pollingQr = false;
    const result = await call<{image: string}>('qr');
    if (!root.isConnected) return;
    el('qr').querySelector('img')!.src = result.image; el('qr').hidden = false;
    feedback('请用手机 QQ 扫码，并在手机上确认。'); pollingQr = true; void checkQr();
  });
  el('sync').onclick = () => void run(async () => { show(await call('sync')); await refresh(); feedback('正在后台同步，可以关闭此面板继续浏览。'); });
  el('local').onclick = () => void run(async () => { show(await call('local')); await refresh(); feedback('已导入本机 QQ 曲库快照。在线播放仍需连接账户。'); });
  el<HTMLInputElement>('enabled').onchange = (e) => void run(async () => { show(await call('enable', {enabled:(e.target as HTMLInputElement).checked})); await refresh(); feedback('显示设置已保存。'); });
  const discover = (operation: string) => run(async () => {
    const result = await call<{albumId: string; count: number}>(operation, {query:el<HTMLInputElement>('query').value.trim()});
    await refresh(); if (result.count) reveal(result.albumId); else feedback('没有找到歌曲。');
  });
  el('search-form').onsubmit = e => { e.preventDefault(); void discover('search'); };
  el('daily').onclick = () => void discover('daily');
  el('key-form').onsubmit = e => { e.preventDefault(); void run(async () => { const key = el<HTMLInputElement>('key'); show(await call('key', {key:key.value})); key.value = ''; feedback('官方 Key 验证成功。'); }); };
  el('remember').onclick = () => void run(async () => { show(await call('remember')); feedback('已保存至 macOS 钥匙串。'); });
  el('logout').onclick = () => void run(async () => { pollingQr=false; el('qr').hidden=true; show(await call('logout')); await refresh(); feedback('连接已移除，QQ 曲库已隐藏。'); });
  void update();
}
