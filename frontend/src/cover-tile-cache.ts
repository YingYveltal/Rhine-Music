/** Bounded shared tiles: logical shelf slots retain their own identities. */
export class CoverTileCache {
  readonly slots: Int32Array;
  private readonly keys: (string | undefined)[];
  private readonly references: Int32Array;
  private readonly tiles = new Map<string, number>();
  constructor(count: number) {
    this.slots = new Int32Array(count).fill(-1);
    this.keys = Array(count);
    this.references = new Int32Array(count);
  }
  lookup(key: string) {
    return this.tiles.get(key);
  }
  key(tile: number) {
    return this.keys[tile];
  }
  get size() {
    return this.tiles.size;
  }
  assign(slot: number, key: string) {
    const old = this.slots[slot];
    if (old >= 0 && this.keys[old] === key) return { tile: old, paint: false };
    if (old >= 0) this.references[old]--;
    let tile = this.tiles.get(key),
      paint = false;
    if (tile === undefined) {
      tile = this.references.findIndex((n) => n === 0);
      if (tile < 0) throw new Error("No unreferenced cover tile");
      const previous = this.keys[tile];
      if (previous !== undefined) this.tiles.delete(previous);
      this.keys[tile] = key;
      this.tiles.set(key, tile);
      paint = true;
    }
    this.references[tile]++;
    this.slots[slot] = tile;
    return { tile, paint };
  }
  reset() {
    this.slots.fill(-1);
    this.keys.fill(undefined);
    this.references.fill(0);
    this.tiles.clear();
  }
}
