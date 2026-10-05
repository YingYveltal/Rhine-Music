import { BokehPass } from "three/addons/postprocessing/BokehPass.js";

// Moments of Three.js's original 41-tap, centrally symmetric bokeh kernel.
// When every tap stays within one texel of its center, bilinear interpolation
// reduces the convolution exactly to the same 3x3 texels. Four bilinear reads
// plus the center reproduce those nine weights; this is not a new blur kernel.
export const bokehKernel = (() => {
  const taps: [number, number][] = [[0, 0]];
  const ring = [
    [0, 0.4],
    [0.15, 0.37],
    [0.29, 0.29],
    [-0.37, 0.15],
    [0.4, 0],
    [0.37, -0.15],
    [0.29, -0.29],
    [-0.15, -0.37],
    [0, -0.4],
    [-0.15, 0.37],
    [-0.29, 0.29],
    [0.37, 0.15],
    [-0.4, 0],
    [-0.37, -0.15],
    [-0.29, -0.29],
    [0.15, -0.37],
  ];
  taps.push(...ring.map(([x, y]) => [x, y] as [number, number]));
  taps.push(
    ...[
      [0.15, 0.37],
      [-0.37, 0.15],
      [0.37, -0.15],
      [-0.15, -0.37],
      [-0.15, 0.37],
      [0.37, 0.15],
      [-0.37, -0.15],
      [0.15, -0.37],
    ].map(([x, y]) => [x * 0.9, y * 0.9] as [number, number]),
  );
  for (const scale of [0.7, 0.4])
    taps.push(
      ...[
        [0.29, 0.29],
        [0.4, 0],
        [0.29, -0.29],
        [0, -0.4],
        [-0.29, 0.29],
        [-0.4, 0],
        [-0.29, -0.29],
        [0, 0.4],
      ].map(([x, y]) => [x * scale, y * scale] as [number, number]),
    );
  const ax = taps.reduce((s, [x]) => s + Math.abs(x), 0) / 41;
  const ay = taps.reduce((s, [, y]) => s + Math.abs(y), 0) / 41;
  const axy = taps.reduce((s, [x, y]) => s + Math.abs(x * y), 0) / 41;
  return {
    taps,
    ax,
    ay,
    axy,
    weight: (ax * ay) / axy,
    offsetX: axy / ay,
    offsetY: axy / ax,
  };
})();
export function optimizeBokeh(pass: BokehPass) {
  const material = pass.materialBokeh;
  material.uniforms.rhineFastBokeh = { value: true };
  const k = bokehKernel;
  material.fragmentShader =
    "uniform bool rhineFastBokeh;\n" + material.fragmentShader;
  material.fragmentShader = material.fragmentShader.replace(
    "vec4 col = vec4( 0.0 );",
    `
    vec2 colorSize = vec2(textureSize(tColor, 0));
    vec2 radius = abs(dofblur * aspectcorrect) * colorSize;
    // Only use the algebraic reduction in the domain where it is exact.
    if (rhineFastBokeh && max(radius.x, radius.y) * 0.4 <= 0.999) {
      vec2 offset = radius * vec2(${k.offsetX.toPrecision(15)},${k.offsetY.toPrecision(15)}) / colorSize;
      vec4 neighbors = texture2D(tColor, vUv + offset)
        + texture2D(tColor, vUv - offset)
        + texture2D(tColor, vUv + vec2(offset.x,-offset.y))
        + texture2D(tColor, vUv + vec2(-offset.x,offset.y));
      gl_FragColor = neighbors * ${(k.weight / 4).toPrecision(15)}
        + texture2D(tColor,vUv) * ${(1 - k.weight).toPrecision(15)};
      gl_FragColor.a = 1.0;
      return;
    }
    vec4 col = vec4( 0.0 );
  `,
  );
  material.needsUpdate = true;
}
