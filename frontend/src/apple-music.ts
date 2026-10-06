import { isNative, nativeInvoke } from './native';
import { applePanelControls, appleStatusText, type AppleStatus } from './apple-status';
import './apple-music.css';

export function mountApplePanel(root: HTMLElement, refresh: () => Promise<void>) {
  root.innerHTML = `<h3>Apple Music</h3><p id="apple-status" role="status">正在读取资料库访问状态…</p>
    <div class="panel-actions"><button id="apple-authorize" disabled>允许访问音乐资料库</button><button class="primary-button" id="apple-sync" disabled>同步我的歌单</button></div>
    <label class="settings-row"><span>在卡片架显示 Apple Music</span><input id="apple-enabled" type="checkbox" disabled></label>
    <p id="apple-feedback" role="status" aria-live="polite"></p>`;
  const el = <T extends HTMLElement = HTMLElement>(id: string) => root.querySelector<T>(`#apple-${id}`)!;
  if (!isNative) {
    el('status').textContent = '请在 macOS 桌面应用中连接 Apple Music。';
    return () => {};
  }
  let disposed = false, busy = false, requestVersion = 0;
  let latest: AppleStatus | undefined, timer: ReturnType<typeof setTimeout> | undefined;
  let libraryStamp: string | undefined;
  let syncFeedbackPending = false;
  const alive = () => !disposed && root.isConnected;
  const call = (operation: string, body: object = {}) => nativeInvoke<AppleStatus>('apple_request', { operation, body });
  const controls = () => {
    const available = applePanelControls(latest, busy);
    el<HTMLButtonElement>('authorize').disabled = !available.authorize;
    el('authorize').hidden = !!latest && (!latest.supported || latest.authorization !== 'notDetermined');
    el<HTMLButtonElement>('sync').disabled = !available.sync;
    el<HTMLInputElement>('enabled').disabled = !available.enable;
    if (latest) el<HTMLInputElement>('enabled').checked = latest.enabled;
  };
  const show = (s: AppleStatus) => {
    latest = s;
    if (!alive()) return;
    el('status').textContent = appleStatusText(s);
    controls();
  };
  const update = async () => {
    if (!alive()) return;
    try {
      if (!busy) {
        const version = ++requestVersion;
        try {
          const s = await call('status');
          if (!alive() || version !== requestVersion) return;
          const syncFinished = syncFeedbackPending && latest?.job.running && !s.job.running;
          show(s);
          // Only clear the progress message still owned by this sync. The main
          // status now supplies the final counts or failure, without erasing a
          // newer display-setting result or unrelated error.
          if (syncFinished) { syncFeedbackPending = false; el('feedback').textContent = ''; }
          const stamp = JSON.stringify([s.enabled, s.updatedAt, s.playlistCount, s.trackCount, s.job.running, s.job.error]);
          const changed = libraryStamp !== undefined && stamp !== libraryStamp;
          libraryStamp = stamp;
          if (changed && !s.job.running) await refresh();
        } catch (error) {
          if (alive() && version === requestVersion) {
            syncFeedbackPending = false;
            el('feedback').textContent = `无法读取资料库状态：${(error as Error).message}`;
          }
        }
      }
    } finally {
      if (alive()) timer = setTimeout(() => void update(), 1500);
    }
  };
  const run = async (operation: 'authorize' | 'sync' | 'enable', body: object = {}) => {
    if (!alive() || busy) return;
    const allowed = applePanelControls(latest, false);
    if (!allowed[operation]) return;
    syncFeedbackPending = false;
    busy = true; ++requestVersion; controls(); root.setAttribute('aria-busy', 'true');
    el('feedback').textContent = operation === 'authorize' ? '正在请求音乐资料库访问权限…' : '正在处理…';
    try {
      const s = await call(operation, body); show(s);
      await refresh();
      if (alive()) {
        syncFeedbackPending = operation === 'sync' && s.job.running;
        el('feedback').textContent = syncFeedbackPending
          ? '正在同步歌单，可以关闭此面板继续浏览。'
          : operation === 'enable' ? '显示设置已保存。' : '';
      }
    } catch (error) {
      if (alive()) el('feedback').textContent = `未能完成操作：${(error as Error).message}`;
    } finally {
      busy = false;
      if (alive()) { root.removeAttribute('aria-busy'); controls(); }
    }
  };
  el('authorize').onclick = () => void run('authorize');
  el('sync').onclick = () => void run('sync');
  el<HTMLInputElement>('enabled').onchange = () => void run('enable', { enabled: el<HTMLInputElement>('enabled').checked });
  void update();
  return () => { disposed = true; ++requestVersion; clearTimeout(timer); };
}
