import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import * as statusHelpers from '../src/apple-status.ts';

// Isolated DOM/IPC boundary tests, not MusicKit or visual acceptance.
const script = ts.transpileModule(readFileSync(new URL('../src/apple-music.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
const base = { supported: true, authorization: 'notDetermined', enabled: true, playlistCount: 0, trackCount: 0, updatedAt: null, job: { running: false, completed: 0, total: 0, message: null, error: null } };
function fixture(native = true) {
  const nodes = new Map(['status', 'authorize', 'sync', 'enabled', 'feedback'].map(id => [`#apple-${id}`, { disabled: true, hidden: false, checked: false, textContent: '' }]));
  const root = { isConnected: true, innerHTML: '', querySelector: id => nodes.get(id), setAttribute() {}, removeAttribute() {} };
  const calls = [], timers = new Map(); let refreshed = 0, nextTimer = 0;
  const exports = {};
  vm.runInNewContext(script, {
    exports,
    require(name) {
      if (name === './apple-status') return statusHelpers;
      if (name.endsWith('.css')) return {};
      assert.equal(name, './native');
      return { isNative: native, nativeInvoke(command, args) {
        assert.equal(command, 'apple_request');
        return new Promise((resolve, reject) => calls.push({ ...args, resolve, reject }));
      } };
    },
    setTimeout(callback) { timers.set(++nextTimer, callback); return nextTimer; },
    clearTimeout(id) { timers.delete(id); },
  });
  const dispose = exports.mountApplePanel(root, async () => { refreshed++; });
  return { calls, nodes, root, dispose, timers, get refreshed() { return refreshed; }, async tick() {
    assert.equal(timers.size, 1);
    const [id, cb] = timers.entries().next().value; timers.delete(id); cb(); await settle();
  } };
}

test('opening reads status only; denied access never retries authorization', async () => {
  const env = fixture();
  assert.deepEqual(env.calls.map(c => c.operation), ['status']);
  env.calls[0].resolve({ ...base, authorization: 'denied' }); await settle();
  assert.equal(env.nodes.get('#apple-authorize').hidden, true);
  env.nodes.get('#apple-authorize').onclick(); await settle();
  assert.equal(env.calls.length, 1);
  env.dispose(); assert.equal(env.timers.size, 0);
});
test('a stale status reply cannot undo authorization; polling continues after invalidation', async () => {
  const env = fixture(); env.calls[0].resolve(base); await settle();
  await env.tick(); const old = env.calls.at(-1);
  env.nodes.get('#apple-authorize').onclick(); await settle();
  assert.equal(env.calls.at(-1).operation, 'authorize');
  const authorization = env.calls.at(-1);
  old.resolve(base); await settle();
  authorization.resolve({ ...base, authorization: 'authorized' }); await settle();
  assert.equal(env.nodes.get('#apple-sync').disabled, false);
  assert.match(env.nodes.get('#apple-status').textContent, /已允许访问/);
  await env.tick(); assert.equal(env.calls.at(-1).operation, 'status');
  env.dispose(); env.calls.at(-1).resolve(base); await settle();
  assert.match(env.nodes.get('#apple-status').textContent, /已允许访问/);
  assert.equal(env.timers.size, 0);
});
test('sync completion refreshes cards, and display toggle sends the selected boolean', async () => {
  const env = fixture(); const authorized = { ...base, authorization: 'authorized' };
  env.calls[0].resolve(authorized); await settle();
  env.nodes.get('#apple-sync').onclick(); await settle();
  assert.equal(env.calls.at(-1).operation, 'sync');
  env.calls.at(-1).resolve({ ...authorized, job: { ...base.job, running: true } }); await settle();
  assert.equal(env.refreshed, 1); assert.equal(env.nodes.get('#apple-sync').disabled, true);
  await env.tick();
  env.calls.at(-1).resolve({ ...authorized, playlistCount: 2, trackCount: 8, updatedAt: '2026-10-07T00:00:00Z' }); await settle();
  assert.equal(env.refreshed, 2); assert.equal(env.nodes.get('#apple-sync').disabled, false);
  env.nodes.get('#apple-enabled').checked = false; env.nodes.get('#apple-enabled').onchange(); await settle();
  assert.equal(env.calls.at(-1).operation, 'enable'); assert.equal(env.calls.at(-1).body.enabled, false);
  env.calls.at(-1).resolve({ ...authorized, enabled: false }); await settle();
  assert.equal(env.nodes.get('#apple-enabled').checked, false);
  env.dispose();
});
test('browser mount never invokes native APIs or claims a connection', () => {
  const env = fixture(false);
  assert.equal(env.calls.length, 0); assert.equal(env.timers.size, 0);
  assert.match(env.nodes.get('#apple-status').textContent, /macOS 桌面应用/);
  env.dispose();
});
