import * as THREE from "three";

/** A plane's support radius for an affine-transformed local box. Unlike a
 * bounding sphere, this remains tight for the thin, wide cassette geometry.
 * Reject only if all eight corners lie strictly outside the same plane. */
export function intersectsTransformedBox(
  frustum: THREE.Frustum, box: THREE.Box3, matrix: THREE.Matrix4,
) {
  const e = matrix.elements;
  const cx = (box.min.x + box.max.x) * 0.5;
  const cy = (box.min.y + box.max.y) * 0.5;
  const cz = (box.min.z + box.max.z) * 0.5;
  const hx = (box.max.x - box.min.x) * 0.5;
  const hy = (box.max.y - box.min.y) * 0.5;
  const hz = (box.max.z - box.min.z) * 0.5;
  const x = e[0]*cx + e[4]*cy + e[8]*cz + e[12];
  const y = e[1]*cx + e[5]*cy + e[9]*cz + e[13];
  const z = e[2]*cx + e[6]*cy + e[10]*cz + e[14];
  for (const plane of frustum.planes) {
    const n = plane.normal;
    const radius = hx * Math.abs(n.x*e[0] + n.y*e[1] + n.z*e[2])
      + hy * Math.abs(n.x*e[4] + n.y*e[5] + n.z*e[6])
      + hz * Math.abs(n.x*e[8] + n.y*e[9] + n.z*e[10]);
    // Retain tangent boxes, with a small margin for floating point roundoff.
    if (n.x*x + n.y*y + n.z*z + plane.constant + radius < -1e-6) return false;
  }
  return true;
}

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
      mesh.geometry.boundingBox ?? mesh.geometry.computeBoundingBox();
      return { mesh, count: mesh.count, matrix, attributes };
    });
  }
  apply(
    camera: THREE.Camera,
    shadow: THREE.Frustum | undefined,
    enabled: boolean,
    precise = false,
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
      const materials = Array.isArray(s.mesh.material) ? s.mesh.material : [s.mesh.material];
      // Unsupported deformation keeps the existing sphere path.
      const box = precise && !s.mesh.geometry.morphAttributes.position?.length
        && materials.every(m => !("displacementMap" in m) || !m.displacementMap)
        ? s.mesh.geometry.boundingBox : null;
      const intersects = (frustum: THREE.Frustum) => frustum.intersectsSphere(this.sphere)
        && (!box || intersectsTransformedBox(frustum, box, this.transform));
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
          !intersects(this.view) &&
          !(s.mesh.castShadow && shadow && intersects(shadow))
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
