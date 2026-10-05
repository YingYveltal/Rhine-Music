import * as THREE from "three";
import type { BokehPass } from "three/addons/postprocessing/BokehPass.js";
import { nativeInvoke } from "./native";

// Explicit, one-shot developer capture. No GPU readbacks or geometry copies run
// during ordinary playback. Rows retain WebGL's bottom-up order in every buffer.
export async function captureForMetal(
  renderer: THREE.WebGLRenderer, scene: THREE.Scene,
  camera: THREE.PerspectiveCamera, bokeh: BokehPass,
  render: () => void, metadata: Record<string, unknown>,
) {
  const id = `${new Date().toISOString().replace(/[^0-9]/g, "")}-${metadata.theme}-${metadata.phase}`;
  const buffers: { name: string; bytes: Uint8Array }[] = [];
  const read = (name: string, target: THREE.WebGLRenderTarget) => {
    const data = new Uint16Array(target.width * target.height * 4);
    renderer.readRenderTargetPixels(target, 0, 0, target.width, target.height, data);
    buffers.push({ name, bytes: new Uint8Array(data.buffer) });
  };
  const original = bokeh.render;
  try {
    bokeh.render = function(r, write, input, ...rest) {
      read("color.rgba16f", input);
      original.call(this, r, write, input, ...rest);
      read("depth.rgba16f", (this as unknown as { _renderTargetDepth: THREE.WebGLRenderTarget })._renderTargetDepth);
      read("bokeh.rgba16f", write);
    };
    render();
    const gl = renderer.getContext();
    const pixels = new Uint8Array(gl.drawingBufferWidth * gl.drawingBufferHeight * 4);
    gl.readPixels(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    buffers.push({ name: "reference.rgba8", bytes: pixels });
  } finally { bokeh.render = original; }

  const geometries: { positions: number[]; indices: number[] }[] = [];
  const geometryIds = new Map<THREE.BufferGeometry, number>();
  const instances: { geometry: number; matrix: number[]; name: string }[] = [];
  const matrix = new THREE.Matrix4();
  scene.traverseVisible(object => {
    if (!(object instanceof THREE.Mesh)) return;
    const material = Array.isArray(object.material) ? object.material[0] : object.material;
    if (!material?.visible || material.opacity === 0) return;
    const geometry = object.geometry;
    if (!geometryIds.has(geometry)) {
      const position = geometry.getAttribute("position");
      if (!position) return;
      const positions: number[] = [];
      for (let i = 0; i < position.count; i++) positions.push(position.getX(i), position.getY(i), position.getZ(i));
      const indices: number[] = geometry.index ? Array.from(geometry.index.array as ArrayLike<number>) : Array.from({ length: position.count }, (_, i) => i);
      geometryIds.set(geometry, geometries.length);
      geometries.push({ positions, indices });
    }
    const add = (transform: THREE.Matrix4) => {
      if (Math.abs(transform.determinant()) < 1e-10) return;
      instances.push({ geometry: geometryIds.get(geometry)!, matrix: transform.toArray(), name: object.name || material.name || material.type });
    };
    if (object instanceof THREE.InstancedMesh) {
      for (let i = 0; i < object.count; i++) { object.getMatrixAt(i, matrix); matrix.premultiply(object.matrixWorld); add(matrix); }
    } else add(object.matrixWorld);
  });
  const manifest = {
    version: 1, id, ...metadata,
    width: renderer.domElement.width, height: renderer.domElement.height,
    rowOrder: "bottom-up", buffers: buffers.map(b => ({ name: b.name, bytes: b.bytes.length })),
    uniforms: Object.fromEntries(["focus", "aspect", "aperture", "maxblur", "nearClip", "farClip", "rhineFastBokeh"].map(k => [k, bokeh.materialBokeh.uniforms[k].value])),
    exposure: renderer.toneMappingExposure, toneMapping: renderer.toneMapping,
    cameraWorld: camera.matrixWorld.toArray(), projection: camera.projectionMatrix.toArray(),
    inverseProjection: camera.projectionMatrixInverse.toArray(), geometries, instances,
    geometryScope: "Visible mesh geometry, actual normalized vertices and current transforms. Visibility benchmark treats all triangles as opaque and double-sided; it does not reproduce glass shading, alpha masks, SSAO, or shadows.",
  };
  for (const buffer of buffers) {
    let binary = "";
    for (let i = 0; i < buffer.bytes.length; i += 32768)
      binary += String.fromCharCode(...buffer.bytes.subarray(i, i + 32768));
    await nativeInvoke("save_render_capture", { id, name: buffer.name, data: btoa(binary) });
  }
  // Manifest is the completion marker; partially written captures are ignored.
  await nativeInvoke("save_render_capture", { id, name: "scene.json", data: btoa(unescape(encodeURIComponent(JSON.stringify(manifest)))) });
  return id;
}
