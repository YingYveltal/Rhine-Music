import assert from "node:assert/strict";
import { CoverTileCache } from "../src/cover-tile-cache.ts";
const cache = new CoverTileCache(432);
let paints = 0;
for (let i = 0; i < 432; i++)
  paints += Number(cache.assign(i, `album-${i % 3}`).paint);
assert.equal(paints, 3);
assert.equal(cache.size, 3);
for (let tick = 0; tick < 100; tick++)
  for (let i = 0; i < 432; i++) {
    const key = `album-${(i + tick) % 3}`;
    assert.equal(cache.assign(i, key).paint, false);
    assert.equal(cache.key(cache.slots[i]), key);
  }
// Full capacity, eviction, repeated references and identity after rapid reversal.
for (let i = 0; i < 432; i++) cache.assign(i, `unique-${i}`);
for (let i = 0; i < 432; i++)
  assert.equal(cache.key(cache.slots[i]), `unique-${i}`);
for (let i = 431; i >= 0; i--) cache.assign(i, `new-${i}`);
for (let i = 0; i < 432; i++)
  assert.equal(cache.key(cache.slots[i]), `new-${i}`);
cache.reset();
assert.equal(cache.size, 0);
assert.ok(cache.slots.every((x) => x === -1));
console.log(
  "Cover cache: 43,200 repeated-slot changes require only 3 initial paints; full-capacity eviction preserves all identities.",
);
