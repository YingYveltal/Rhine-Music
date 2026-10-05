import assert from "node:assert/strict";
import { bokehKernel as k } from "../src/bokeh-optimization.ts";
import { BokehShader } from "three/addons/shaders/BokehShader.js";
const original = [
  [0, 0],
  ...Array.from(
    BokehShader.fragmentShader.matchAll(
      /vec2\(\s*([-.\d]+),\s*([-.\d]+)\s*\) \* aspectcorrect \) \* dofblur([974]?)/g,
    ),
    (m) => [
      Number(m[1]) * (m[3] ? Number(m[3]) / 10 : 1),
      Number(m[2]) * (m[3] ? Number(m[3]) / 10 : 1),
    ],
  ),
];
assert.equal(original.length, 41);
assert.deepEqual(k.taps, original);
let seed = 1129;
const random = () => (seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32;
for (let test = 0; test < 10000; test++) {
  const grid = Array.from({ length: 9 }, () => random() * 4);
  const sample = (x, y) => {
    const ix = Math.floor(x),
      iy = Math.floor(y),
      fx = x - ix,
      fy = y - iy;
    const at = (i, j) => grid[(j + 1) * 3 + i + 1];
    return (
      at(ix, iy) * (1 - fx) * (1 - fy) +
      at(ix + 1, iy) * fx * (1 - fy) +
      at(ix, iy + 1) * (1 - fx) * fy +
      at(ix + 1, iy + 1) * fx * fy
    );
  };
  // Avoid accessing an irrelevant out-of-range texel at the center's exact zero.
  const read = (x, y) =>
    Math.abs(x) + Math.abs(y) < 1e-15 ? grid[4] : sample(x, y);
  const rx = random() * 2.497,
    ry = random() * 2.497;
  const before = k.taps.reduce((s, [x, y]) => s + read(x * rx, y * ry), 0) / 41;
  const ox = rx * k.offsetX,
    oy = ry * k.offsetY;
  const after =
    ((read(ox, oy) + read(-ox, -oy) + read(ox, -oy) + read(-ox, oy)) *
      k.weight) /
      4 +
    grid[4] * (1 - k.weight);
  assert.ok(Math.abs(before - after) < 1e-12, `${before} != ${after}`);
}
console.log(
  "Bokeh: 10,000 random HDR neighborhoods match the original 41 taps within 1e-12.",
);
