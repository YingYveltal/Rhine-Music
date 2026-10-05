import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rgbaRows, mergeUpdates } from '../src/metal-data.ts';
const row=(...values)=>new Uint8Array(values.flatMap(v=>[v,v,v,255]));
test('canvas rows preserve WebGL flip, crop, and byte order',()=>{
 const data=row(1,2,3,4,5,6,7,8,9);
 assert.deepEqual(rgbaRows(data,2,2,3,1,1,false),row(5,6,8,9));
 assert.deepEqual(rgbaRows(data,2,2,3,1,1,true),row(8,9,5,6));
 assert.throws(()=>rgbaRows(data,4,1,3),/exceeds/);
});
test('discarding a frame retains overlapping deltas in last-write order',()=>{
 const first=[{key:'whole',v:1},{key:'middle',v:2}];
 const next=[{key:'whole',v:3},{key:'end',v:4}];
 assert.deepEqual(mergeUpdates(first,next,x=>x.key),[{key:'middle',v:2},{key:'whole',v:3},{key:'end',v:4}]);
 assert.equal(first[0].v,1);
});
