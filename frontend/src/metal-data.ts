// Keep upload row order identical to WebGL, including a flipped canvas source.
export function rgbaRows(
  source: Uint8Array | Uint8ClampedArray,
  width: number,
  height: number,
  rowPixels = width,
  skipPixels = 0,
  skipRows = 0,
  flip = false,
): Uint8Array {
  const stride = rowPixels * 4;
  if (
    width < 0 ||
    height < 0 ||
    skipPixels < 0 ||
    skipRows < 0 ||
    skipPixels + width > rowPixels ||
    ((skipRows + height - 1) * rowPixels + skipPixels + width) * 4 >
      source.length
  )
    throw new Error("Texture upload exceeds source pixels");
  const result = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    const row = skipRows + (flip ? height - 1 - y : y);
    result.set(
      source.subarray(
        row * stride + skipPixels * 4,
        row * stride + (skipPixels + width) * 4,
      ),
      y * width * 4,
    );
  }
  return result;
}
export function encodeBytes(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 32768)
    binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
  return btoa(binary);
}
// Only replace identical ranges. Keep overlapping ranges in their last-write order.
export function mergeUpdates<T>(
  previous: T[],
  latest: T[],
  key: (value: T) => string,
): T[] {
  const updates = new Map<string, T>();
  for (const item of [...previous, ...latest]) {
    const k = key(item);
    updates.delete(k);
    updates.set(k, item);
  }
  return [...updates.values()];
}
