import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import vm from 'node:vm';

const source = stripTypeScriptTypes(readFileSync(new URL('../src/document-decryption.ts', import.meta.url),'utf8'), {mode:'transform'})
  .replace('export class ', 'class ');
function fixture(batched) {
  let dirty=false, flushes=0;
  const read=()=>{if(dirty){flushes++;dirty=false;}};
  const write=()=>{dirty=true;};
  const rect=(left,top,width,height)=>({left,top,right:left+width,bottom:top+height,width,height});
  const targets=Array.from({length:24},(_,i)=>{
    const classes=new Set(); const left=30+i,top=40+i*30;
    return {children:[], classList:{add(v){if(!classes.has(v)){classes.add(v);write();}}},
      getBoundingClientRect(){read();return rect(left,top,200,40);},get offsetWidth(){read();return 100;},get clientWidth(){read();return 100;},
      nodes:[{textContent:'one',rects:[rect(left,top,100,18),rect(left+102,top+1,80,18),rect(left+4,top+20,150,18)]},{textContent:' '}],
      append(el){this.children.push(el);el.parent=this;write();},
    };
  });
  const root={querySelectorAll:()=>targets};
  const document={
    createTreeWalker(target){let i=0;return {nextNode:()=>target.nodes[i++]??null};},
    createRange(){let node;return {selectNodeContents(n){node=n;},getClientRects(){read();return node.rects;}};},
    createElement(){return {className:'',style:{},children:[],setAttribute(){},append(el){this.children.push(el);},remove(){if(this.parent){this.parent.children.splice(this.parent.children.indexOf(this),1);write();}}};},
  };
  const ctx=vm.createContext({document,NodeFilter:{SHOW_TEXT:4}});
  vm.runInContext(`${source}; globalThis.controller = new DocumentDecryption('text',0.35);`,ctx);
  const controller=ctx.controller; controller.batchLayout=batched;
  const snapshot=()=>targets.map(t=>t.children.map(w=>({rect:w.style.cssText,transform:w.children[0].style.transform})));
  return {controller,root,snapshot,get flushes(){return flushes;}};
}
test('batched reads preserve wrapped/scaled overlay geometry while avoiding repeated layout invalidations',()=>{
  const a=fixture(false),b=fixture(true);a.controller.reset(a.root,false);b.controller.reset(b.root,false);
  assert.deepEqual(b.snapshot(),a.snapshot());
  assert.equal(b.snapshot().flat().length,48);
  assert.equal(b.flushes,1);assert.ok(a.flushes>=24);
  for(const t of [0,0.35,0.55,0.9]){
    a.controller.update(t,{clarity:0},false);b.controller.update(t,{clarity:0},false);
    assert.deepEqual(b.snapshot(),a.snapshot());
  }
  a.controller.update(2,{clarity:1},false);b.controller.update(2,{clarity:1},false);
  assert.deepEqual(b.snapshot(),a.snapshot());assert.equal(b.snapshot().flat().length,0);
});
test('refresh replaces old overlays; reduced motion remains clear',()=>{
  const f=fixture(true);f.controller.reset(f.root,false);const before=f.snapshot();f.controller.refresh();
  assert.deepEqual(f.snapshot(),before);assert.equal(f.snapshot().flat().length,48);
  f.controller.reset(f.root,true);assert.equal(f.snapshot().flat().length,0);
});
