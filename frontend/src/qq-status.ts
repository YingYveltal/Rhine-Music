export interface QqStatus {
  connected: boolean; officialConfigured: boolean; enabled: boolean; remembered?: boolean;
  connectionState?: string; connectionNotice?: string | null; storedConnection?: boolean; loginPending?: boolean;
  playlistCount: number; trackCount: number; updatedAt: string;
  job: { running: boolean; completed?: number; total?: number; message?: string; error?: string };
}

export function qqStatusText(s: QqStatus) {
  const state = s.connectionState === 'checking' ? '正在验证连接，请稍候。'
    : s.connectionState === 'unverified' ? '连接尚未验证，请点“重试验证连接”。'
    : s.connectionState === 'expired' ? '登录已过期，请重新扫码。'
    : s.connected ? 'QQ 音乐已连接，可以同步自己的曲库。'
    : '尚未连接 QQ 音乐，请扫码连接自己的曲库。';
  const lines = [state];
  if (s.remembered) lines.push('连接已保存到本机，下次打开会自动恢复并验证。');
  else if (s.connected) lines.push('本次连接尚未保存；退出后需重新扫码。');
  else if (s.storedConnection) lines.push('本机仍有保存的连接；验证状态见上方，也可选择忘记连接。');
  if (s.loginPending) lines.push('扫码尚未完成，请在手机上确认；也可以取消本次扫码。');
  if (s.connectionNotice) lines.push(s.connectionNotice);
  lines.push(`已缓存 ${s.playlistCount} 个歌单 / 专辑，${s.trackCount} 个曲目条目。`);
  if (s.job.running) lines.push(`正在同步曲库：${s.job.message || '正在读取歌曲'}（${s.job.completed ?? 0}/${s.job.total || '…'}）`);
  else if (s.job.error) lines.push(`同步未完成：${s.job.error}。已有曲库保留；连接正常后可再点“同步我的曲库”。`);
  else if (s.updatedAt) lines.push(`上次同步：${new Date(s.updatedAt).toLocaleString('zh-CN')}`);
  return lines.join('\n');
}

export function qqDiscoveryText(s: QqStatus) {
  return {
    search: s.connected ? '搜索结果的播放以账户和歌曲权限为准。' : '可以直接尝试搜索，播放前请先扫码连接。',
    daily: s.officialConfigured ? '每日推荐服务已配置。' : '每日推荐需先在“高级设置”配置；扫码连接和个人曲库播放不需要这项配置。',
    advanced: s.officialConfigured ? '官方 API Key 已配置，用于官方搜索和每日推荐。' : '官方 API Key 未配置；在线搜索仍会尝试现有搜索服务，不影响扫码连接和账户允许的播放。',
  };
}
