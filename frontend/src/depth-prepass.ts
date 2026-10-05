import * as THREE from "three";
import { RenderPass } from "three/addons/postprocessing/RenderPass.js";

/** Populate opaque depth before expensive glass shading. Colour, transmission
 * capture and shadow rendering still use the unmodified materials/pipeline. */
export class DepthPrepass extends RenderPass {
  optimized = true;
  private depth = new THREE.MeshDepthMaterial({ side: THREE.DoubleSide });
  constructor(scene: THREE.Scene, camera: THREE.Camera) {
    super(scene, camera);
    this.depth.colorWrite = false;
  }
  override render(
    renderer: THREE.WebGLRenderer,
    write: THREE.WebGLRenderTarget,
    read: THREE.WebGLRenderTarget,
    delta: number,
    mask: boolean,
  ) {
    if (!this.optimized) {
      super.render(renderer, write, read, delta, mask);
      return;
    }
    const scene = this.scene;
    const saved = {
      background: scene.background,
      override: scene.overrideMaterial,
      autoClear: renderer.autoClear,
      depth: renderer.autoClearDepth,
      clear: this.clear,
      shadowAuto: renderer.shadowMap.autoUpdate,
      shadowNeeds: renderer.shadowMap.needsUpdate,
    };
    const hidden: THREE.Object3D[] = [];
    try {
      scene.traverse((object) => {
        if (!object.visible) return;
        if (object instanceof THREE.Mesh) {
          const materials = Array.isArray(object.material)
            ? object.material
            : [object.material];
          // Leave alpha-cut prints, fading copies and displaced geometry to the
          // original pass. A depth-only substitute cannot reproduce their mask.
          const safe = materials.every(
            (m) =>
              m instanceof THREE.MeshStandardMaterial &&
              m.side === THREE.DoubleSide &&
              m.visible &&
              !m.transparent &&
              m.opacity === 1 &&
              m.depthWrite &&
              m.depthTest &&
              !m.map &&
              !m.alphaMap &&
              !m.alphaTest &&
              !m.displacementMap &&
              !m.clippingPlanes,
          );
          if (!safe) {
            hidden.push(object);
            object.visible = false;
          }
        } else if (
          object instanceof THREE.Points ||
          object instanceof THREE.Line ||
          object instanceof THREE.Sprite
        ) {
          hidden.push(object);
          object.visible = false;
        }
      });
      renderer.setRenderTarget(this.renderToScreen ? null : read);
      renderer.clear(true, true, true);
      renderer.autoClear = false;
      renderer.shadowMap.autoUpdate = false;
      renderer.shadowMap.needsUpdate = false;
      scene.background = null;
      scene.overrideMaterial = this.depth;
      renderer.render(scene, this.camera);
      for (const object of hidden) object.visible = true;
      scene.background = saved.background;
      scene.overrideMaterial = saved.override;
      renderer.shadowMap.autoUpdate = saved.shadowAuto;
      renderer.shadowMap.needsUpdate = saved.shadowNeeds;
      // A Color background forces clearing even when autoClear is false.
      // Preserve the prefilled depth, while retaining the original clear colour.
      renderer.autoClearDepth = false;
      this.clear = false;
      super.render(renderer, write, read, delta, mask);
    } finally {
      for (const object of hidden) object.visible = true;
      scene.background = saved.background;
      scene.overrideMaterial = saved.override;
      renderer.autoClear = saved.autoClear;
      renderer.autoClearDepth = saved.depth;
      this.clear = saved.clear;
      renderer.shadowMap.autoUpdate = saved.shadowAuto;
      // The beauty pass has consumed its requested shadow update.
    }
  }
  override dispose() {
    this.depth.dispose();
  }
}
