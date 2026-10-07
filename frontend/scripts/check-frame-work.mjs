import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import vm from 'node:vm';

// Run the real probe with a deterministic clock and no browser/GPU/native IPC.
const source = stripTypeScriptTypes(readFileSync(new URL('../src/performance-probe.ts', import.meta.url), 'utf8'))
  .replace(/import \{ isNative, nativeInvoke \} from "\.\/native";/, 'const isNative = false;')
  .replaceAll('export function ', 'function ');
function fixture() {
  let now = 1000;
  const document = { hidden: false, documentElement: { dataset: {} } };
  const context = vm.createContext({ performance: { now: () => now }, document, innerWidth: 1280, innerHeight: 788,
    devicePixelRatio: 2, navigator: { userAgent: 'probe-fixture' }, console: { info() {} } });
  vm.runInContext(`${source}\nglobalThis.probe = {beginMeasurement, sampleFrame, measurementActive, recordLibraryRefresh, beginFrameWork, measureWork};`, context);
  const scene = { measureFrame: false, frameWork: undefined,
    measurementCounters: () => ({ outgoing: 0, coverCache: { printHits: 1, printMisses: 2 } }),
    renderer: { domElement: {width: 1920, height: 1182}, info: {render: {calls: 50, triangles: 750000}} },
    motionResolution: {enabled: false, scale: 1}, nativeMetal: {stats: {}}, getStats: () => ({}),
    postFusionEnabled: false, transmissionDepthEnabled: false,
  };
  return { ...context.probe, document, scene, setNow(t) { now=t; }, report() { return JSON.parse(document.documentElement.dataset.measurement); } };
}
test('frame work is scoped to measurement, keeps timestamps, and shuts off on completion', () => {
  const f=fixture(); assert.equal(f.scene.measureFrame,false);
  assert.equal(f.beginMeasurement('work',1,{},f.scene),true);
  assert.equal(f.scene.measureFrame,true);
  f.sampleFrame(1000,2,f.scene);
  f.scene.frameWork={frameTime:2,updateMs:1,atlasUploadMs:2,resolutionMs:3,renderSubmissionMs:4,resized:true,outgoing:2};
  f.setNow(2000); f.sampleFrame(2000,11,f.scene);
  const r=f.report(); assert.equal(r.valid,true); assert.equal(f.scene.measureFrame,false); assert.equal(f.measurementActive(),false);
  assert.equal(r.raw[0].atMs,1000); assert.equal(r.raw[0].work.renderSubmissionMs,4);
  assert.equal(r.gpuMs,null); assert.equal(r.startingCounters.coverCache.printMisses,2);
  assert.match(r.workTimingNote,/Neither is GPU/);
});
test('a stale scene sample from a prior callback is not attributed to the current frame', () => {
  const f=fixture(); f.beginMeasurement('stale',1,{},f.scene); f.sampleFrame(1000,2,f.scene);
  f.scene.frameWork={frameTime:1.5,updateMs:200};
  f.setNow(2000); f.sampleFrame(2000,3,f.scene);
  assert.equal(f.report().raw[0].work,undefined);
});
test('library refresh markers capture comparison CPU time and counts only during a run', () => {
  const f=fixture(); f.recordLibraryRefresh(99,true,123);
  f.beginMeasurement('library',1,{},f.scene); f.sampleFrame(1000,2,f.scene);
  f.setNow(1250); f.recordLibraryRefresh(7,false,24);
  f.setNow(2000); f.sampleFrame(2000,3,f.scene);
  assert.deepEqual(f.report().libraryRefreshes,[{atMs:250,comparisonCpuMs:7,changed:false,albums:24}]);
  f.recordLibraryRefresh(99,true,123); assert.equal(f.report().libraryRefreshes.length,1);
});

test('nested CPU spans keep inclusive time and between-frame work stays separate', () => {
  const f=fixture();
  assert.equal(f.measureWork('inactive',()=>42),42);
  f.beginMeasurement('spans',1,{},f.scene);f.sampleFrame(1000,0,f.scene);
  f.setNow(1100);f.measureWork('event',()=>f.setNow(1103));
  f.beginFrameWork();f.setNow(1900);
  f.measureWork('parent',()=>{f.setNow(1902);f.measureWork('child',()=>f.setNow(1910));f.setNow(1915);});
  f.setNow(2000);f.sampleFrame(2000,15,f.scene);
  assert.deepEqual(f.report().betweenFrameWork,[{name:'event',atMs:100,cpuMs:3}]);
  assert.deepEqual(f.report().raw[0].appWork,[{name:'child',atMs:902,cpuMs:8},{name:'parent',atMs:900,cpuMs:15}]);
});
test('exceptions propagate and retain a CPU span; hidden frames are invalid', () => {
  const f=fixture();f.beginMeasurement('exception',1,{},f.scene);
  f.beginFrameWork();f.setNow(1100);
  assert.throws(()=>f.measureWork('throw',()=>{f.setNow(1105);throw Error('expected');}),/expected/);
  f.document.hidden=true;f.sampleFrame(1105,5,f.scene);
  f.document.hidden=false;f.sampleFrame(1200,0,f.scene);
  f.setNow(2000);f.sampleFrame(2000,0,f.scene);
  assert.equal(f.report().valid,false);assert.equal(f.report().raw[0].appWork,undefined);
});
