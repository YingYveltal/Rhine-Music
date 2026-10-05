import * as THREE from "three";

/** Three renders the back of each double-sided glass mesh into its transmission
 * capture. For a repeated solid mesh, first resolve its nearest back surface
 * using depth only; hidden instances then skip expensive physical shading.
 * The capture's resolved colour/mipmaps and the material shader stay unchanged. */
export class TransmissionPrepass {
  enabled = true;
  batch = false;
  private prepared = new Set<THREE.WebGLRenderTarget | null>();
  private depth = new THREE.MeshDepthMaterial({ side: THREE.BackSide, colorWrite: false });
  private original: THREE.WebGLRenderer["renderBufferDirect"];
  beginFrame() { this.prepared.clear(); }
  private safe(object: THREE.Object3D, material: THREE.Material, geometry: THREE.BufferGeometry) {
    return object instanceof THREE.Mesh && material instanceof THREE.MeshPhysicalMaterial
      && material.transmission > 0 && !material.transparent && material.opacity === 1
      && material.depthWrite && material.depthTest && material.depthFunc === THREE.LessEqualDepth
      && !material.map && !material.alphaMap && !material.alphaTest && !material.displacementMap
      && !material.polygonOffset && !material.clippingPlanes && !this.renderer.clippingPlanes.length
      && !geometry.morphAttributes.position?.length && !(object instanceof THREE.SkinnedMesh);
  }
  constructor(private renderer: THREE.WebGLRenderer) {
    this.original = renderer.renderBufferDirect;
    renderer.renderBufferDirect = (camera, scene, geometry, material, object, group) => {
      const safe = this.enabled && material.side === THREE.BackSide && this.safe(object,material,geometry);
      if (safe && this.batch && !this.prepared.has(renderer.getRenderTarget())) {
        this.prepared.add(renderer.getRenderTarget());
        // Three's back surfaces all sample the SAME resolved opaque texture;
        // the texture is resolved again only after the entire back-face list.
        // Their colour therefore depends on the nearest surviving depth, not on
        // hidden predecessors. Preserve colour order/ties; reject hidden shading.
        scene.traverseVisible(candidate => {
          if (!(candidate instanceof THREE.Mesh) || Array.isArray(candidate.material)
            || !candidate.layers.test(camera.layers) || !candidate.material.visible
            || ![THREE.DoubleSide,THREE.BackSide].includes(candidate.material.side)
            || !this.safe(candidate,candidate.material,candidate.geometry)) return;
          candidate.modelViewMatrix.multiplyMatrices(camera.matrixWorldInverse,candidate.matrixWorld);
          candidate.normalMatrix.getNormalMatrix(candidate.modelViewMatrix);
          this.original.call(renderer,camera,scene,candidate.geometry,this.depth,candidate,{start:0,count:Infinity,materialIndex:0});
        });
      } else if (safe && !this.batch && object instanceof THREE.InstancedMesh)
        this.original.call(renderer, camera, scene, geometry, this.depth, object, group);
      this.original.call(renderer, camera, scene, geometry, material, object, group);
    };
  }
  dispose() {
    this.renderer.renderBufferDirect = this.original;
    this.depth.dispose();
  }
}
