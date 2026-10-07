import type { MusicAlbum, MusicTrack } from './music-types';

// Old saved Apple snapshots have no kind and contain playlists only.
export const isApplePlaylist = (a: MusicAlbum) => a.source === 'apple' && a.kind !== 'album';
export const appleCollectionLabel = (a: MusicAlbum) => isApplePlaylist(a) ? 'Apple Music 歌单' : 'Apple Music 专辑';
const positiveNumber = (n?: number) => n && Number.isInteger(n) && n > 0 ? n : undefined;
export function appleDiscSummary(a: MusicAlbum) {
  const discs = [...new Set(a.tracks.map(t => positiveNumber(t.discNumber)).filter((n): n is number => n !== undefined))].sort((a, b) => a - b);
  const unknown = a.tracks.some(t => !positiveNumber(t.discNumber));
  return [discs.length ? `DISC ${discs.map(n => String(n).padStart(2, '0')).join(' / ')}` : '', unknown || !discs.length ? '部分或全部碟号未提供' : ''].filter(Boolean).join(' · ');
}
export function appleTrackNumber(a: MusicAlbum, t: MusicTrack, index: number) {
  const n = isApplePlaylist(a) ? index + 1 : positiveNumber(t.trackNumber);
  return n ? String(n).padStart(2, '0') : '—';
}
export function appleDiscHeading(a: MusicAlbum, index: number) {
  if (isApplePlaylist(a)) return '';
  const discs = a.tracks.map(t => positiveNumber(t.discNumber));
  const number = discs[index];
  if (discs.every(n => n === 1) || (index > 0 && number === discs[index - 1])) return '';
  return number ? `DISC ${String(number).padStart(2, '0')}` : '碟号未提供';
}
