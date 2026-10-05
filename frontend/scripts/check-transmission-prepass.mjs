import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from 'three';
import { TransmissionPrepass } from '../src/transmission-prepass.ts';

const geometry = () => new THREE.BoxGeometry(1,1,.1);
const glass = (options={}) => new THREE.MeshPhysicalMaterial({transmission:.9,side:THREE.DoubleSide,...options});
function fixture() {
  const calls=[], target={};
  const renderer={clippingPlanes:[],getRenderTarget:()=>target,
    renderBufferDirect:(camera,scene,geometry,material,object,group)=>calls.push({material,object,group})};
  const pass=new TransmissionPrepass(renderer);pass.batch=true;
  const scene=new THREE.Scene(),camera=new THREE.PerspectiveCamera();
  const front=new THREE.Mesh(geometry(),glass()),back=new THREE.Mesh(geometry(),glass());
  front.position.z=-2;back.position.z=-4;scene.add(front,back);scene.updateMatrixWorld();camera.updateMatrixWorld();
  const draw=(object)=>{const side=object.material.side;object.material.side=THREE.BackSide;
    renderer.renderBufferDirect(camera,scene,object.geometry,object.material,object,null);object.material.side=side;};
  return {pass,scene,camera,front,back,calls,draw};
}
test('whole-list back depth precedes shading and is submitted once per frame',()=>{
  const f=fixture();f.draw(f.front);f.draw(f.back);
  assert.deepEqual(f.calls.map(c=>[c.material.colorWrite,c.object===f.front?'front':'back']),
    [[false,'front'],[false,'back'],[true,'front'],[true,'back']]);
  assert.equal(f.back.modelViewMatrix.elements[14],-4);
  assert.equal(f.calls[1].group.count,Infinity);
  f.pass.beginFrame();f.draw(f.front);assert.equal(f.calls.filter(c=>!c.material.colorWrite).length,4);
});
test('masked, fading, invisible, alternate-layer and displaced surfaces retain their original depth behavior',()=>{
  const f=fixture();
  for (const mat of [glass({alphaTest:.1}),glass({transparent:true}),glass({opacity:.5}),glass({depthWrite:false}),glass({displacementMap:new THREE.Texture()})])
    f.scene.add(new THREE.Mesh(geometry(),mat));
  const hidden=new THREE.Group();hidden.visible=false;hidden.add(new THREE.Mesh(geometry(),glass()));f.scene.add(hidden);
  const other=new THREE.Mesh(geometry(),glass());other.layers.set(2);f.scene.add(other);
  f.scene.updateMatrixWorld();f.draw(f.front);
  assert.equal(f.calls.filter(c=>!c.material.colorWrite).length,2);
});
test('legacy per-instance prepass remains available for the comparison path',()=>{
  const f=fixture();f.pass.batch=false;f.draw(f.front);assert.equal(f.calls.length,1);
  const mesh=new THREE.InstancedMesh(geometry(),glass(),2);f.scene.add(mesh);f.draw(mesh);
  assert.deepEqual(f.calls.map(c=>c.material.colorWrite),[true,false,true]);
});
