import test from 'node:test';
import assert from 'node:assert/strict';
import { appendMusicFolders } from '../src/library-folders.ts';
import { qqStatusText, qqDiscoveryText } from '../src/qq-status.ts';

test('folder cancellation and duplicates preserve unsaved text byte for byte', () => {
  const draft = '  /Music/Existing/  \n\n/Music/手动输入';
  assert.deepEqual(appendMusicFolders(draft, null), { value: draft, added: 0 });
  assert.deepEqual(appendMusicFolders(draft, ['/Music/Existing', '/Music/手动输入/']), { value: draft, added: 0 });
});
test('multiple folders append once without replacing manual entries or merging distinct names', () => {
  assert.deepEqual(appendMusicFolders('/Music/manual\n', ['/Music/New', '/Music/New/', '/Music/new', '/Music/中文 空格']), {
    value: '/Music/manual\n/Music/New\n/Music/new\n/Music/中文 空格', added: 3,
  });
  assert.deepEqual(appendMusicFolders('', ['/Music/one']), { value: '/Music/one', added: 1 });
});
const base = { connected: false, officialConfigured: false, enabled: true, playlistCount: 2, trackCount: 8, updatedAt: '', job: { running: false } };
test('cached music or a configured Key alone never implies an account connection', () => {
  for (const officialConfigured of [false, true]) {
    const text = qqStatusText({ ...base, officialConfigured });
    assert.match(text, /尚未连接/);
    assert.match(text, /已缓存 2.*8/);
    assert.doesNotMatch(text, /Key|API|本次连接尚未保存/);
  }
});
test('verified connections clearly distinguish saved and session-only login', () => {
  assert.match(qqStatusText({ ...base, connected: true }), /本次连接尚未保存/);
  const saved = qqStatusText({ ...base, connected: true, remembered: true });
  assert.match(saved, /QQ 音乐已连接/);
  assert.match(saved, /下次打开会自动恢复并验证/);
  assert.doesNotMatch(saved, /Key|未保存/);
});
test('checking, unverified and expired states do not claim verified playback', () => {
  for (const [connectionState, expected] of [['checking', /正在验证/], ['unverified', /重试验证连接/], ['expired', /登录已过期/]]) {
    const text = qqStatusText({ ...base, connectionState, remembered: true });
    assert.match(text, expected);
    assert.doesNotMatch(text, /QQ 音乐已连接/);
  }
  assert.match(qqStatusText({ ...base, storedConnection: true }), /仍有保存的连接/);
});
test('sync progress and real failures remain visible alongside connection notices', () => {
  assert.match(qqStatusText({ ...base, job: { running: true, completed: 2, total: 5, message: '正在同步收藏' } }), /正在同步收藏（2\/5）/);
  const text = qqStatusText({ ...base, connectionNotice: '网络不可用，请重试', job: { running: false, error: '服务返回 503' } });
  assert.match(text, /网络不可用，请重试/);
  assert.match(text, /同步未完成：服务返回 503/);
  assert.match(text, /已有曲库保留/);
});
test('search stays available without a Key, while daily points to advanced configuration', () => {
  const hints = qqDiscoveryText(base);
  assert.match(hints.search, /直接尝试/);
  assert.match(hints.search, /播放前请先扫码/);
  assert.match(hints.daily, /高级设置/);
  assert.doesNotMatch(hints.daily, /API|Key/);
  assert.match(hints.advanced, /API Key 未配置/);
  assert.match(qqDiscoveryText({ ...base, connected: true }).daily, /扫码连接和个人曲库播放不需要/);
  assert.match(qqDiscoveryText({ ...base, officialConfigured: true }).daily, /服务已配置/);
});
