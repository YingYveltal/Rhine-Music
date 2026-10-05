import test from 'node:test';
import assert from 'node:assert/strict';
import { MotionResolution } from '../src/motion-resolution.ts';
test('original quality is the default, and small targets are not reduced',()=>{
  const q=new MotionResolution();q.update(0,[0],true,2e6);
  assert.equal(q.update(.1,[1],true,2e6),1);
  q.enabled=true;assert.equal(q.update(.2,[2],true,1e6),1);
});
test('motion lowers only one tier, holds through brief gaps, then restores full resolution',()=>{
  const q=new MotionResolution();q.enabled=true;q.update(0,[0],true,2e6);
  assert.equal(q.update(.1,[1],true,2e6),2/3);
  assert.equal(q.update(.8,[1],true,2e6),2/3);
  assert.equal(q.update(1,[1],true,2e6),1);
  assert.equal(q.update(1.1,[1.0001],true,2e6),1);
});
test('native preparation and switching off restore the original tier immediately',()=>{
  const q=new MotionResolution();q.enabled=true;q.update(0,[0],true,2e6);q.update(.1,[1],true,2e6);
  assert.equal(q.update(.2,[2],false,2e6),1);
  q.enabled=false;assert.equal(q.update(.3,[3],true,2e6),1);
  q.reset();assert.equal(q.scale,1);
});
