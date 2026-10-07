import assert from "node:assert/strict";
import * as THREE from "three";
import { InstanceVisibility, intersectsTransformedBox } from "../src/instance-visibility.ts";
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

// Compare the support-radius formula to all eight corners, including rotation,
// non-uniform scale, mirrored scale and shear. This checks conservative rejection.
let seed = 123;
const random = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 2**32);
const box = new THREE.Box3(new THREE.Vector3(-2.2, .2, -.07), new THREE.Vector3(2.2, 3.5, .07));
const frustum = new THREE.Frustum().setFromProjectionMatrix(camera.projectionMatrix);
for (let trial = 0; trial < 1000; trial++) {
  const transform = new THREE.Matrix4().compose(
    new THREE.Vector3(random()*30-15, random()*20-10, -random()*40),
    new THREE.Quaternion().setFromEuler(new THREE.Euler(random()*6, random()*6, random()*6)),
    new THREE.Vector3(random()*3-1.5, random()*3, random()*3),
  );
  transform.elements[4] += random()*.2;
  const corners = [];
  for (const x of [box.min.x,box.max.x]) for (const y of [box.min.y,box.max.y]) for (const z of [box.min.z,box.max.z])
    corners.push(new THREE.Vector3(x,y,z).applyMatrix4(transform));
  const reference = frustum.planes.every(p => Math.max(...corners.map(c => p.distanceToPoint(c))) >= -1e-6);
  assert.equal(intersectsTransformedBox(frustum,box,transform),reference);
}
const thin = new THREE.InstancedMesh(new THREE.BoxGeometry(.1, 10, .1), new THREE.MeshBasicMaterial(), 2);
thin.setMatrixAt(0,new THREE.Matrix4().makeTranslation(0,0,-5));
thin.setMatrixAt(1,new THREE.Matrix4().makeTranslation(5,0,-5));
thin.updateMatrixWorld();
const tight = new InstanceVisibility();tight.capture([thin]);
tight.apply(camera,undefined,true,false); assert.equal(thin.count,2);
tight.apply(camera,undefined,true,true); assert.equal(thin.count,1);
assert.equal(tight.logicalSlot(thin,0),0);
// A caster outside the camera must survive if any part is in the light frustum.
thin.castShadow=true;
lightCamera.position.x=5;lightCamera.updateMatrixWorld();
shadow.setFromProjectionMatrix(new THREE.Matrix4().multiplyMatrices(lightCamera.projectionMatrix,lightCamera.matrixWorldInverse));
tight.apply(camera,shadow,true,true);assert.equal(thin.count,2);
tight.apply(camera,undefined,true,false);assert.equal(thin.count,2);
console.log('Precise box support matches eight-corner rejection for 1000 affine transforms; shadow union and toggle restoration remain correct.');
