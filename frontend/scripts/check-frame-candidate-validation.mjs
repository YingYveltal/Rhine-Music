import test from 'node:test';
import assert from 'node:assert/strict';
import {captureFrameCandidatePixels} from '../src/frame-candidate-validation.ts';

test('visual diagnostic changes only one flag, checks A/A and A/B, and restores enabled entry state',()=>{
  let enabled=true;const reads=[];
  const r=captureFrameCandidatePixels({enabled:()=>enabled,setEnabled:v=>{enabled=v;},
    read:()=>{reads.push(enabled);return new Uint8Array([10,enabled?22:20,30,255]);}});
  assert.deepEqual(reads,[false,true,false,false,true,false,true]);
  assert.equal(enabled,true);assert.equal(r.pixelGate,true);assert.equal(r.ab.maximum,2);
});
test('unstable A/A or a three-level colour difference cannot pass',()=>{
  let enabled=false,calls=0;
  const run=unstable=>captureFrameCandidatePixels({enabled:()=>enabled,setEnabled:v=>{enabled=v;},
    read:()=>new Uint8Array([unstable?++calls:(enabled?3:0),0,0,255])});
  assert.equal(run(false).pixelGate,false);assert.equal(run(true).pixelGate,false);
  assert.equal(enabled,false);
});
test('readback failure restores the original flag and does not return a passing report',()=>{
  let enabled=false,calls=0;
  assert.throws(()=>captureFrameCandidatePixels({enabled:()=>enabled,setEnabled:v=>{enabled=v;},
    read:()=>{if(++calls===5)throw new Error('readback failed');return new Uint8Array([0,0,0,255]);}}),/readback failed/);
  assert.equal(enabled,false);
});
