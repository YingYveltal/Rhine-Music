import test from 'node:test';
import assert from 'node:assert/strict';
import { createQqLogin, validationMessage } from '../src/qq-login.ts';

test('a successful validate RPC with expired cookies does not claim connection success', () => {
  const expired = { connected: false, connectionState: 'disconnected', connectionNotice: '保存的登录已过期或无法恢复，请重新扫码；已同步曲库仍保留' };
  assert.equal(validationMessage(expired), expired.connectionNotice);
  assert.match(validationMessage({ connected: false, connectionState: 'expired' }), /过期/);
  assert.match(validationMessage({ connected: true, connectionState: 'checking' }), /仍在验证/);
  assert.equal(validationMessage({ connected: true, connectionState: 'connected' }), '连接验证通过。');
});

const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const settle = () => new Promise(resolve => setImmediate(resolve));
function harness() {
  const calls = [], events = [];
  const call = (operation, body) => {
    const reply = deferred(); calls.push({ operation, body, ...reply });
    if (operation === 'cancel_login') reply.resolve({});
    return reply.promise;
  };
  const login = createQqLogin(call, {
    image: image => events.push(['image', image]), message: text => events.push(['message', text]),
    connected: () => events.push(['connected']),
  });
  return { calls, events, login };
}

test('closing before reservation reply cancels that id without fetching a QR', async () => {
  const h = harness(); const starting = h.login.start(); h.login.dispose();
  h.calls[0].resolve({ attemptId: 10 }); await starting;
  assert.deepEqual(h.calls.map(c => c.operation), ['login_start', 'cancel_login']);
  assert.equal(h.calls[1].body.attemptId, 10); assert.deepEqual(h.events, []);
});

test('closing during QR request ignores late image and stops polling', async () => {
  const h = harness(); const starting = h.login.start(); h.calls[0].resolve({ attemptId: 10 }); await settle();
  h.login.dispose(); h.calls[1].resolve({ image: 'synthetic-image' }); await starting;
  assert.deepEqual(h.calls.map(c => c.operation), ['login_start', 'qr', 'cancel_login']);
  assert.deepEqual(h.events, []);
});

test('cancel during polling ignores late success and never triggers sync callback', async () => {
  const h = harness(); const starting = h.login.start(); h.calls[0].resolve({ attemptId: 10 }); await settle();
  h.calls[1].resolve({ image: 'synthetic-image' }); await starting;
  h.login.cancel(); const before = h.events.length;
  h.calls[2].resolve({ connected: true }); await settle();
  assert.equal(h.events.length, before);
  assert.equal(h.calls.filter(c => c.operation === 'poll').length, 1); h.login.dispose();
});

test('replacement attempt is unaffected by an older success', async () => {
  const h = harness(); const first = h.login.start(); h.calls[0].resolve({ attemptId: 10 }); await settle();
  h.calls[1].resolve({ image: 'old' }); await first;
  const second = h.login.start(); h.calls[4].resolve({ attemptId: 11 }); await settle();
  h.calls[5].resolve({ image: 'new' }); await second;
  h.calls[2].resolve({ connected: true }); await settle();
  assert.equal(h.events.filter(e => e[0] === 'connected').length, 0);
  h.calls[6].resolve({ connected: true }); await settle();
  assert.equal(h.events.filter(e => e[0] === 'connected').length, 1); h.login.dispose();
});

for (const outcome of ['expired', 'cancelled', 'failure']) {
  test(`${outcome} terminates polling and permits a fresh attempt`, async () => {
    const h = harness(); const first = h.login.start(); h.calls[0].resolve({ attemptId: 10 }); await settle();
    h.calls[1].resolve({ image: 'synthetic' }); await first;
    if (outcome === 'failure') h.calls[2].reject(new Error('模拟网络超时'));
    else h.calls[2].resolve({ [outcome]: true, message: outcome });
    await settle();
    assert.equal(h.calls.at(-1).operation, 'cancel_login');
    assert.equal(h.events.filter(e => e[0] === 'connected').length, 0);
    const retry = h.login.start(); const reserved = h.calls.at(-1); h.login.dispose();
    reserved.resolve({ attemptId: 11 }); await retry;
    assert.equal(h.calls.at(-1).body.attemptId, 11);
  });
}

test('waiting schedules one poll and disposal clears the timer', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const h = harness(); const starting = h.login.start(); h.calls[0].resolve({ attemptId: 10 }); await settle();
  h.calls[1].resolve({ image: 'synthetic' }); await starting;
  h.calls[2].resolve({ message: 'waiting' }); await settle();
  t.mock.timers.tick(2000); await settle();
  assert.equal(h.calls.filter(c => c.operation === 'poll').length, 2);
  h.calls[3].resolve({ message: 'confirming' }); await settle(); h.login.dispose();
  t.mock.timers.tick(2000); await settle();
  assert.equal(h.calls.filter(c => c.operation === 'poll').length, 2);
});
