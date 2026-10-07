export interface AppleStatus {
  supported: boolean;
  authorization: 'notDetermined' | 'authorized' | 'denied' | 'restricted';
  enabled: boolean;
  playlistCount: number;
  albumCount?: number;
  trackCount: number;
  unconfirmedPlaylistCount?: number;
  unconfirmedAlbumCount?: number;
  updatedAt: string | null;
  unavailableReason?: string | null;
  job: { running: boolean; completed: number; total: number; message: string | null; error: string | null };
}

export function appleStatusText(s: AppleStatus) {
  if (!s.supported) return s.unavailableReason || 'Apple Music 需要 macOS 14 或更新版本；本地音乐和 QQ 音乐仍可使用。';
  const lines = [s.authorization === 'authorized'
    ? '已允许访问音乐资料库。'
    : s.authorization === 'denied'
      ? '尚未允许访问。请在系统设置的“隐私与安全性”中允许 Rhine Music 访问音乐资料库，再回来重试。'
      : s.authorization === 'restricted'
        ? '音乐资料库访问受到系统限制。请检查系统的隐私或使用限制设置。'
        : '允许访问音乐资料库后，可以同步自己的 Apple Music 专辑和歌单。'];
  const counts = [s.albumCount ? `${s.albumCount} 张专辑` : '', s.playlistCount ? `${s.playlistCount} 个歌单` : ''].filter(Boolean);
  if (counts.length) lines.push(`已保存 ${counts.join('、')}，${s.trackCount} 个曲目条目。`);
  if (s.job.running) lines.push(`正在同步：${s.job.message || '读取我的资料库'}（${s.job.completed}/${s.job.total || '…'}）`);
  else if (s.job.error) lines.push(`同步未完成：${s.job.error}。已有资料库保留，可以稍后重试。`);
  else if (s.updatedAt) {
    if (!counts.length) lines.push('资料库中暂时没有专辑或歌单。可在“音乐”App 中收藏专辑或添加歌单后重新同步。');
    const date = new Date(s.updatedAt);
    if (Number.isFinite(date.getTime())) lines.push(`上次同步：${date.toLocaleString('zh-CN')}`);
  } else if (s.authorization === 'authorized') lines.push('尚未同步资料库，请点击“同步我的资料库”。');
  const uncertain = [s.unconfirmedAlbumCount ? `${s.unconfirmedAlbumCount} 张专辑` : '', s.unconfirmedPlaylistCount ? `${s.unconfirmedPlaylistCount} 个歌单` : ''].filter(Boolean);
  if (!s.job.running && uncertain.length) {
    lines.push(`其中 ${uncertain.join('、')}本次未读取到歌曲。若本来为空可忽略；若已有歌曲，请稍后重新同步。`);
  }
  return lines.join('\n');
}

export function applePanelControls(s: AppleStatus | undefined, busy: boolean) {
  return {
    authorize: !!s?.supported && s.authorization === 'notDetermined' && !busy && !s.job.running,
    sync: !!s?.supported && s.authorization === 'authorized' && !busy && !s.job.running,
    enable: !!s?.supported && !busy,
  };
}
