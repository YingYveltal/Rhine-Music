import assert from "node:assert/strict";
import * as THREE from "three";
import { InstanceVisibility } from "../src/instance-visibility.ts";
const mesh = new THREE.InstancedMesh(
  new THREE.BoxGeometry(1, 1, 1),
  new THREE.MeshBasicMaterial(),
  3,
);
const matrix = new THREE.Matrix4();
for (const [i, x] of [0, 100, 200].entries()) {
  matrix.makeTranslation(x, 0, -5);
  mesh.setMatrixAt(i, matrix);
}
mesh.geometry.setAttribute(
  "coverTile",
  new THREE.InstancedBufferAttribute(new Float32Array([11, 22, 33]), 1),
);
const camera = new THREE.PerspectiveCamera(60, 1, 0.1, 100);
camera.updateMatrixWorld();
mesh.updateMatrixWorld();
const c = new InstanceVisibility();
c.capture([mesh]);
c.apply(camera, undefined, true);
assert.equal(mesh.count, 1);
assert.equal(c.logicalSlot(mesh, 0), 0);
assert.equal(mesh.geometry.getAttribute("coverTile").array[0], 11);
// A visible shadow caster is kept even when it is outside the viewing frustum.
mesh.castShadow = true;
const lightCamera = new THREE.PerspectiveCamera(60, 1, 0.1, 100);
lightCamera.position.x = 100;
lightCamera.updateMatrixWorld();
const shadow = new THREE.Frustum().setFromProjectionMatrix(
  new THREE.Matrix4().multiplyMatrices(
    lightCamera.projectionMatrix,
    lightCamera.matrixWorldInverse,
  ),
);
c.apply(camera, shadow, true);
assert.equal(mesh.count, 2);
assert.equal(c.logicalSlot(mesh, 1), 1);
c.apply(camera, undefined, false);
assert.equal(mesh.count, 3);
assert.deepEqual(
  [...mesh.geometry.getAttribute("coverTile").array],
  [11, 22, 33],
);
console.log(
  "Instance culling preserves off-screen shadow casters, logical picking IDs and per-instance cover coordinates.",
);
// Near-to-far reordering must retain picking and per-instance artwork identity.
for (const [i,z] of [-20,-3,-8].entries()) mesh.setMatrixAt(i,matrix.makeTranslation(0,0,z));
c.capture([mesh]);c.apply(camera,undefined,true);
assert.deepEqual([0,1,2].map(i=>c.logicalSlot(mesh,i)),[1,2,0]);
assert.deepEqual([...mesh.geometry.getAttribute('coverTile').array],[22,33,11]);
const ray=new THREE.Raycaster();ray.setFromCamera(new THREE.Vector2(),camera);
const hit=ray.intersectObject(mesh)[0];assert.equal(c.logicalSlot(mesh,hit.instanceId),1);
c.restore();assert.deepEqual([...mesh.geometry.getAttribute('coverTile').array],[11,22,33]);
console.log('Opaque depth order keeps the same nearest picked album and UVs.');
