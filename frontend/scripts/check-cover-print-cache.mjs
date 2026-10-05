import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from 'three';
import { CoverAtlas } from '../src/cover-atlas.ts';
// These tests exercise texture ownership. Pixel fidelity is checked separately
// against the native/WebGL image oracle; no renderer is substituted here.
globalThis.document={createElement:()=>({width:0,height:0,getContext:()=>new Proxy({}, {get:()=>()=>{},set:()=>true})})};
const record=id=>({id,title:String(id),album:{id}});
test('returning to an album reuses its uploaded texture without changing pixels',async()=>{
 const atlas=new CoverAtlas(16,4096,8);
 await atlas.select(record('a'));const a=atlas.selected.material.map,version=a.version;
 await atlas.select(record('b'));assert.notEqual(atlas.selected.material.map,a);
 await atlas.select(record('a'));assert.equal(atlas.selected.material.map,a);assert.equal(a.version,version);
 assert.equal(atlas.cacheStats.printHits,1);atlas.dispose();
});
test('eviction protects an outgoing cover until its animation releases it',async()=>{
 const atlas=new CoverAtlas(16,4096,8);await atlas.select(record('a'));
 const a=atlas.selected.material.map;let disposals=0;a.addEventListener('dispose',()=>disposals++);
 const outgoing=new THREE.Mesh();atlas.snapshot(outgoing);
 assert.equal(outgoing.material.map,a);
 for(let i=0;i<30;i++)await atlas.select(record(i));
 assert.equal(disposals,0);assert.ok(atlas.cacheStats.printedCovers<=12);
 outgoing.userData.releaseCoverPrint();outgoing.userData.releaseCoverPrint();
 await atlas.select(record('new'));assert.equal(disposals,1);
 atlas.dispose();assert.equal(disposals,1);
});
test('library reset retires old covers but preserves a currently animating snapshot',async()=>{
 const atlas=new CoverAtlas(16,4096,8);await atlas.select(record('a'));
 const a=atlas.selected.material.map;let disposals=0;a.addEventListener('dispose',()=>disposals++);
 const outgoing=new THREE.Mesh();atlas.snapshot(outgoing);
 atlas.reset();await atlas.select(record('b'));assert.equal(disposals,0);
 outgoing.userData.releaseCoverPrint();assert.equal(disposals,1);atlas.dispose();
});
