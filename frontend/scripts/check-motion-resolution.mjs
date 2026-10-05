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

test('ambient archive motion cannot repeatedly lower resolution after input stops',()=>{
  for(const fps of [30,60]) {
    const q=new MotionResolution();q.enabled=true;
    q.update(0,[0],true,1920*1182);
    assert.equal(q.update(1/fps,[1],true,1920*1182),2/3);
    // The scene's existing idle state starts once user interaction has ended.
    // Its ambient wave still moves the composed camera/model pose. That motion
    // must not re-arm the interactive resolution tier, at either frame cadence.
    for(let frame=0;frame<fps*20;frame++) {
      const t=3+frame/fps;
      assert.equal(q.update(t,[1+.2*Math.sin(t*2)],true,1920*1182,true),1,
        `ambient frame ${frame} at ${fps} Hz must remain full resolution`);
    }
    // A new interaction exits idle and must still receive the lower tier.
    assert.equal(q.update(24,[3],true,1920*1182,false),2/3);
    assert.equal(q.update(25,[3],true,1920*1182,false),1);
  }
});
