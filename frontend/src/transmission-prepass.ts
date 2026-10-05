import * as THREE from "three";

/** Three renders the back of each double-sided glass mesh into its transmission
 * capture. For a repeated solid mesh, first resolve its nearest back surface
 * using depth only; hidden instances then skip expensive physical shading.
 * The capture's resolved colour/mipmaps and the material shader stay unchanged. */
export class TransmissionPrepass {
  enabled = true;
  private depth = new THREE.MeshDepthMaterial({ side: THREE.BackSide, colorWrite: false });
  private original: THREE.WebGLRenderer["renderBufferDirect"];
  constructor(private renderer: THREE.WebGLRenderer) {
    this.original = renderer.renderBufferDirect;
    renderer.renderBufferDirect = (camera, scene, geometry, material, object, group) => {
      const safe = this.enabled && object instanceof THREE.InstancedMesh
        && material instanceof THREE.MeshPhysicalMaterial && material.transmission > 0
        && material.side === THREE.BackSide && !material.transparent && material.opacity === 1
        && material.depthWrite && material.depthTest && material.depthFunc === THREE.LessEqualDepth
        && !material.map && !material.alphaMap && !material.alphaTest && !material.displacementMap
        && !material.polygonOffset && !material.clippingPlanes && !renderer.clippingPlanes.length
        && !geometry.morphAttributes.position?.length;
      if (safe) this.original.call(renderer, camera, scene, geometry, this.depth, object, group);
      this.original.call(renderer, camera, scene, geometry, material, object, group);
    };
  }
  dispose() {
    this.renderer.renderBufferDirect = this.original;
    this.depth.dispose();
  }
}
