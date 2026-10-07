import assert from 'node:assert/strict';
import test from 'node:test';
import { isApplePlaylist, appleCollectionLabel, appleDiscSummary, appleTrackNumber, appleDiscHeading } from '../src/apple-collection.ts';
const album = { source: 'apple', kind: 'album', tracks: [{ discNumber: 1, trackNumber: 4 }, { discNumber: 1, trackNumber: 7 }, { discNumber: 2, trackNumber: 3 }] };
test('library albums keep real disc/track numbers and never renumber a partial collection', () => {
  assert.equal(isApplePlaylist(album), false);
  assert.equal(appleCollectionLabel(album), 'Apple Music 专辑');
  assert.deepEqual(album.tracks.map((t, i) => appleTrackNumber(album, t, i)), ['04', '07', '03']);
  assert.deepEqual(album.tracks.map((_, i) => appleDiscHeading(album, i)), ['DISC 01', '', 'DISC 02']);
  assert.equal(appleDiscSummary(album), 'DISC 01 / 02');
  const onlySecond = { ...album, tracks: [album.tracks[2]] };
  assert.equal(appleDiscSummary(onlySecond), 'DISC 02');
  assert.equal(appleDiscHeading(onlySecond, 0), 'DISC 02');
});
test('unknown numbers remain unknown; known neighboring metadata is retained', () => {
  const a = { ...album, tracks: [{}, {}, { discNumber: 2, trackNumber: 5 }, { discNumber: 0, trackNumber: 0 }] };
  assert.deepEqual(a.tracks.map((t, i) => appleTrackNumber(a, t, i)), ['—', '—', '05', '—']);
  assert.deepEqual(a.tracks.map((_, i) => appleDiscHeading(a, i)), ['碟号未提供', '', 'DISC 02', '碟号未提供']);
  assert.equal(appleDiscSummary(a), 'DISC 02 · 部分或全部碟号未提供');
});
test('old and new playlist snapshots retain sequence numbering and no disc groups', () => {
  for (const kind of [undefined, 'playlist']) {
    const a = { ...album, kind };
    assert.equal(isApplePlaylist(a), true);
    assert.equal(appleCollectionLabel(a), 'Apple Music 歌单');
    assert.deepEqual(a.tracks.map((t, i) => appleTrackNumber(a, t, i)), ['01', '02', '03']);
    assert.deepEqual(a.tracks.map((_, i) => appleDiscHeading(a, i)), ['', '', '']);
  }
});
