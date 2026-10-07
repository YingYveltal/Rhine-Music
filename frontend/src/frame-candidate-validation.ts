/** Single-variable visual diagnostic only. No animation updates between these reads. */
export function compareFrameCandidatePixels(a: Uint8Array, b: Uint8Array) {
  if (!a.length || a.length !== b.length) throw new Error("Mismatched pixel buffers");
  let maximum = 0, total = 0, aboveTwo = 0;
  for (let i = 0; i < a.length; i++) {
    const difference = Math.abs(a[i] - b[i]);
    maximum = Math.max(maximum, difference);
    total += difference;
    if (difference > 2) aboveTwo++;
  }
  return { maximum, mean: total / a.length, fractionAboveTwo: aboveTwo / a.length };
}

export function captureFrameCandidatePixels(io: {
  enabled: () => boolean;
  setEnabled: (enabled: boolean) => void;
  read: () => Uint8Array;
}) {
  const original = io.enabled();
  const frames: { name: string; enabled: boolean; pixels: Uint8Array }[] = [];
  try {
    // Compile/warm both paths without advancing scene time, before comparison.
    for (const enabled of [false, true]) { io.setEnabled(enabled); io.read(); }
    for (const [name, enabled] of [["A1", false], ["A2", false], ["B", true], ["A3", false]] as const) {
      io.setEnabled(enabled);
      frames.push({ name, enabled, pixels: io.read() });
    }
    const aa = compareFrameCandidatePixels(frames[0].pixels, frames[1].pixels);
    const ab = compareFrameCandidatePixels(frames[1].pixels, frames[2].pixels);
    const restored = compareFrameCandidatePixels(frames[0].pixels, frames[3].pixels);
    return { frames, aa, ab, restored,
      pixelGate: aa.maximum === 0 && restored.maximum === 0 && ab.maximum <= 2 };
  } finally {
    io.setEnabled(original);
    io.read();
  }
}

/** Readback is bottom-up; the saved PNG has ordinary top-down image coordinates. */
export function frameCandidatePng(pixels: Uint8Array, width: number, height: number) {
  if (pixels.length !== width * height * 4) throw new Error("Invalid RGBA dimensions");
  const canvas = document.createElement("canvas");
  canvas.width = width; canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Missing image encoder");
  const image = context.createImageData(width, height);
  for (let row = 0; row < height; row++) {
    const from = (height - 1 - row) * width * 4;
    image.data.set(pixels.subarray(from, from + width * 4), row * width * 4);
  }
  context.putImageData(image, 0, 0);
  return canvas.toDataURL("image/png");
}
