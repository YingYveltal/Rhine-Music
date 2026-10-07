import assert from 'node:assert/strict';
import test from 'node:test';
import { appleStatusText, applePanelControls } from '../src/apple-status.ts';
const status = { supported: true, authorization: 'notDetermined', enabled: true, playlistCount: 0, trackCount: 0, updatedAt: null, job: { running: false, completed: 0, total: 0, message: null, error: null } };

test('authorization is explicit and denied/restricted access cannot repeatedly request permission', () => {
  assert.deepEqual(applePanelControls(undefined, false), { authorize: false, sync: false, enable: false });
  assert.equal(applePanelControls(status, false).authorize, true);
  for (const authorization of ['denied', 'restricted']) {
    const s = { ...status, authorization };
    assert.equal(applePanelControls(s, false).authorize, false);
    assert.equal(applePanelControls(s, false).sync, false);
    assert.match(appleStatusText(s), /系统/);
  }
  assert.deepEqual(applePanelControls(status, true), { authorize: false, sync: false, enable: false });
});
test('authorized, never synced, empty and failed-with-cache are distinct; none promises playable songs', () => {
  const authorized = { ...status, authorization: 'authorized' };
  assert.match(appleStatusText(authorized), /已允许访问/);
  assert.match(appleStatusText(authorized), /已允许访问音乐资料库/);
  assert.match(appleStatusText(authorized), /尚未同步/);
  assert.doesNotMatch(appleStatusText(authorized), /资料库中暂时没有/);
  assert.match(appleStatusText({ ...authorized, updatedAt: '2026-10-07T01:00:00Z' }), /资料库中暂时没有专辑或歌单/);
  const failed = { ...authorized, playlistCount: 2, trackCount: 20, job: { ...status.job, error: '网络不可用' } };
  assert.match(appleStatusText(failed), /2 个歌单/);
  assert.match(appleStatusText(failed), /网络不可用.*已有资料库保留/);
  assert.doesNotMatch(appleStatusText(failed), /没有歌单|token|会员|探针/);
});
test('unsupported systems and a running sync cannot offer unauthorized actions', () => {
  const unsupported = { ...status, supported: false, unavailableReason: '当前系统不支持 Apple Music。' };
  assert.equal(appleStatusText(unsupported), unsupported.unavailableReason);
  assert.deepEqual(applePanelControls(unsupported, false), { authorize: false, sync: false, enable: false });
  const running = { ...status, authorization: 'authorized', job: { ...status.job, running: true, completed: 2, total: 5, message: '读取歌单' } };
  assert.match(appleStatusText(running), /读取歌单（2\/5）/);
  assert.equal(applePanelControls(running, false).sync, false);
});

test('unconfirmed empty playlists remain visible without claiming a complete sync', () => {
  const s = { ...status, authorization: 'authorized', playlistCount: 2, trackCount: 3,
    unconfirmedPlaylistCount: 1, updatedAt: '2026-10-07T01:00:00Z' };
  const text = appleStatusText(s);
  assert.match(text, /已保存 2 个歌单，3 个曲目/);
  assert.match(text, /1 个歌单本次未读取到歌曲/);
  assert.match(text, /本来为空可忽略.*已有歌曲.*稍后重新同步/);
  assert.doesNotMatch(text, /完整同步成功|读取错误|已有资料库保留/);
  assert.equal(applePanelControls(s, false).sync, true);
});

test('mixed and album-only libraries distinguish counts and uncertainty', () => {
  const albums = { ...status, authorization: 'authorized', albumCount: 3, trackCount: 7, updatedAt: '2026-10-07T01:00:00Z' };
  assert.match(appleStatusText(albums), /已保存 3 张专辑，7 个曲目/);
  assert.doesNotMatch(appleStatusText(albums), /暂时没有|尚未同步/);
  const mixed = { ...albums, playlistCount: 2, unconfirmedAlbumCount: 1, unconfirmedPlaylistCount: 1 };
  assert.match(appleStatusText(mixed), /已保存 3 张专辑、2 个歌单/);
  assert.match(appleStatusText(mixed), /1 张专辑、1 个歌单本次未读取到歌曲/);
});
