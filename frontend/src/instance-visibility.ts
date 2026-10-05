import * as THREE from "three";

type Saved = {
  mesh: THREE.InstancedMesh;
  count: number;
  matrix: Float32Array;
  attributes: {
    attribute: THREE.InstancedBufferAttribute;
    values: Float32Array;
  }[];
};
/** Conservative per-instance culling; off-screen shadow casters stay in the light frustum. */
export class InstanceVisibility {
  private saved: Saved[] = [];
  private view = new THREE.Frustum();
  private projection = new THREE.Matrix4();
  private transform = new THREE.Matrix4();
  private sphere = new THREE.Sphere();
  stats = { submitted: 0, total: 0 };
  restore() {
    for (const s of this.saved) {
      s.mesh.count = s.count;
      s.mesh.instanceMatrix.array.set(s.matrix);
      s.mesh.instanceMatrix.needsUpdate = true;
      for (const a of s.attributes) {
        a.attribute.array.set(a.values);
        a.attribute.needsUpdate = true;
      }
      delete s.mesh.userData.rhineVisibleSlots;
    }
  }
  capture(meshes: THREE.InstancedMesh[]) {
    this.saved = meshes.map((mesh) => {
      const previous = this.saved.find((s) => s.mesh === mesh);
      const matrix =
        previous?.matrix ?? new Float32Array(mesh.instanceMatrix.array.length);
      matrix.set(mesh.instanceMatrix.array);
      const attributes = Object.values(mesh.geometry.attributes)
        .filter(
          (a): a is THREE.InstancedBufferAttribute =>
            a instanceof THREE.InstancedBufferAttribute,
        )
        .map((attribute) => {
          const values =
            previous?.attributes.find((a) => a.attribute === attribute)
              ?.values ?? new Float32Array(attribute.array.length);
          values.set(attribute.array);
          return { attribute, values };
        });
      if (mesh.instanceColor) {
        const attribute = mesh.instanceColor;
        const values =
          previous?.attributes.find((a) => a.attribute === attribute)?.values ??
          new Float32Array(attribute.array.length);
        values.set(attribute.array);
        attributes.push({ attribute, values });
      }
      mesh.geometry.boundingSphere ?? mesh.geometry.computeBoundingSphere();
      return { mesh, count: mesh.count, matrix, attributes };
    });
  }
  apply(
    camera: THREE.Camera,
    shadow: THREE.Frustum | undefined,
    enabled: boolean,
  ) {
    this.restore();
    this.stats = { submitted: 0, total: 0 };
    camera.updateMatrixWorld();
    this.view.setFromProjectionMatrix(
      this.projection.multiplyMatrices(
        camera.projectionMatrix,
        camera.matrixWorldInverse,
      ),
    );
    for (const s of this.saved) {
      this.stats.total += s.count;
      if (!enabled) {
        this.stats.submitted += s.count;
        continue;
      }
      const visible: { slot: number; depth: number }[] = [];
      for (let i = 0; i < s.count; i++) {
        this.transform.fromArray(s.matrix, i * 16);
        // Zero scale is the original ownership handoff's deliberately hidden instance.
        if (
          this.transform.elements[0] === 0 &&
          this.transform.elements[1] === 0 &&
          this.transform.elements[2] === 0
        )
          continue;
        this.transform.premultiply(s.mesh.matrixWorld);
        this.sphere
          .copy(s.mesh.geometry.boundingSphere!)
          .applyMatrix4(this.transform);
        if (
          !this.view.intersectsSphere(this.sphere) &&
          !(s.mesh.castShadow && shadow?.intersectsSphere(this.sphere))
        )
          continue;
        const view = camera.matrixWorldInverse.elements,
          c = this.sphere.center;
        visible.push({
          slot: i,
          depth: -(view[2] * c.x + view[6] * c.y + view[10] * c.z + view[14]),
        });
      }
      // Opaque/transmissive shells write depth. Submit nearer instances first,
      // letting early depth rejection skip the hidden PBR fragments. Retain
      // original order for alpha-blended materials, where order affects colour.
      const materials = Array.isArray(s.mesh.material)
        ? s.mesh.material
        : [s.mesh.material];
      if (
        materials.every(
          (material) => !material.transparent && material.depthWrite,
        )
      )
        visible.sort((a, b) => a.depth - b.depth || a.slot - b.slot);
      const slots = visible.map((item) => item.slot);
      for (let j = 0; j < slots.length; j++) {
        const i = slots[j];
        s.mesh.instanceMatrix.array.set(
          s.matrix.subarray(i * 16, i * 16 + 16),
          j * 16,
        );
        for (const a of s.attributes) {
          const n = a.attribute.itemSize;
          a.attribute.array.set(a.values.subarray(i * n, i * n + n), j * n);
        }
      }
      s.mesh.count = slots.length;
      s.mesh.userData.rhineVisibleSlots = slots;
      this.stats.submitted += slots.length;
      s.mesh.instanceMatrix.needsUpdate = true;
      for (const a of s.attributes) a.attribute.needsUpdate = true;
    }
  }
  logicalSlot(object: THREE.Object3D, index: number) {
    return (
      (object.userData.rhineVisibleSlots as number[] | undefined)?.[index] ??
      index
    );
  }
}
