import assert from 'node:assert/strict';
import test from 'node:test';

const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const tracks = ['A', 'B', 'C'].map((id, i) => ({
  id, title: `Title ${id}`, albumId: 'synthetic', artist: 'Fixture',
  duration: 100 + i * 10, format: 'WAV', browserPlayable: true,
}));
const snapshot = (index = 0, appliedCommand = 1, extra = {}) => ({
  track: tracks[index], elapsed: 25, playing: true, transport: 'playing',
  error: null, bgmPlaying: false, bgmError: null, appliedCommand, ...extra,
});

async function fixture(t) {
  const previous = new Map(['window', 'document', 'setTimeout', 'clearTimeout'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  const timers = new Map(), commands = [], polls = [];
  let timerId = 0;
  globalThis.document = { hidden: false, addEventListener() {}, removeEventListener() {} };
  globalThis.window = { addEventListener() {}, __TAURI__: { core: {
    convertFileSrc: path => path,
    invoke(command, args) {
      const call = deferred();
      if (command === 'player_command') commands.push({ ...call, ...args });
      else if (command === 'player_state') polls.push(call);
      else throw new Error(`Unexpected IPC: ${command}`);
      return call.promise;
    },
  } } };
  globalThis.setTimeout = callback => { timers.set(++timerId, callback); return timerId; };
  globalThis.clearTimeout = id => timers.delete(id);
  const { NativeMusicPlayer } = await import(`../src/native.ts?fixture=${Math.random()}`);
  const player = new NativeMusicPlayer();
  t.after(() => {
    player.dispose();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  await settle();
  assert.equal(commands[0].operation, 'settings');
  commands[0].resolve(1);
  polls.shift().resolve(snapshot());
  await settle();
  player.setQueue(tracks);
  return {
    player, commands, polls,
    async poll(state, error) {
      assert.equal(timers.size, 1);
      const [id, callback] = timers.entries().next().value;
      timers.delete(id); callback(); await settle();
      const call = polls.shift();
      if (error) call.reject(error); else call.resolve(state);
      await settle();
    },
    async holdPoll() {
      const [id, callback] = timers.entries().next().value;
      timers.delete(id); callback(); await settle();
      return polls.shift();
    },
  };
}
function selected(player, index, transport = 'loading', time = 0) {
  const s = player.state;
  assert.equal(s.currentTrack.id, tracks[index].id);
  assert.equal(s.currentTrack.title, tracks[index].title);
  assert.equal(s.currentIndex, index);
  assert.equal(s.duration, tracks[index].duration);
  assert.equal(s.currentTime, time);
  assert.equal(s.transport, transport);
  assert.equal(s.loading, transport === 'loading');
  assert.equal(s.playing, transport === 'playing');
  assert.equal(s.error, null);
}

test('next keeps B then C across IPC delay and queued-but-unapplied old polls', async t => {
  const env = await fixture(t), { player, commands } = env;
  const first = player.next(); await settle();
  // Check selection before metadata assertions so the original lost-next bug is explicit.
  await env.poll(snapshot());
  assert.equal(player.state.currentTrack.id, 'B');
  selected(player, 1);
  await env.poll(null, new Error('old state failure before enqueue reply'));
  selected(player, 1);
  commands[1].resolve(2); await first;
  await env.poll(snapshot()); // enqueue reply is NOT a worker acknowledgment
  selected(player, 1);
  await env.poll(null, new Error('old state failure after enqueue reply'));
  selected(player, 1);
  const second = player.next(); await settle();
  assert.deepEqual(commands.filter(c => c.operation === 'play').map(c => c.id), ['B', 'C']);
  selected(player, 2);
  commands[2].resolve(3); await second;
  await env.poll(snapshot(1, 2)); selected(player, 2);
  await env.poll(snapshot(2, 3, { elapsed: 0 })); selected(player, 2, 'playing');
});

test('polls begun before a new intent cannot overwrite it with success or error', async t => {
  const env = await fixture(t), { player, commands } = env;
  let old = await env.holdPoll();
  const b = player.next(); await settle();
  old.resolve(snapshot()); await settle(); selected(player, 1);
  commands[1].resolve(2); await b;
  old = await env.holdPoll();
  const c = player.next(); await settle();
  old.reject(new Error('obsolete poll')); await settle(); selected(player, 2);
  commands[2].resolve(3); await c;
  await env.poll(snapshot(2, 3, { elapsed: 7 })); selected(player, 2, 'playing', 7);
});

test('A to B to A requires the newest command confirmation, not a matching ID', async t => {
  const env = await fixture(t), { player, commands } = env;
  const b = player.next(); await settle();
  const a = player.play('A');
  commands[1].resolve(2); await b; await settle();
  commands[2].resolve(3); await a;
  await env.poll(snapshot(0, 1)); selected(player, 0);
  await env.poll(snapshot(1, 2)); selected(player, 0);
  await env.poll(snapshot(0, 3, { elapsed: 26 })); selected(player, 0, 'playing', 26);
  // Automatic progression carries the same acknowledged user command.
  await env.poll(snapshot(1, 3, { elapsed: 0 })); selected(player, 1, 'playing');
  await env.poll(snapshot(2, 3, { elapsed: 0 })); selected(player, 2, 'playing');
  await env.poll(snapshot(2, 3, { elapsed: 0, playing: false, transport: 'idle' }));
  selected(player, 2, 'idle');
});

test('obsolete command failure cannot poison a newer selection; latest failure can retry', async t => {
  const env = await fixture(t), { player, commands } = env;
  const b = player.next(); await settle();
  const c = player.next();
  commands[1].reject(new Error('obsolete B command')); await b; await settle();
  selected(player, 2);
  commands[2].reject(new Error('C command failed')); await c;
  assert.equal(player.state.transport, 'error');
  assert.equal(player.state.playing, false);
  assert.equal(player.state.loading, false);
  assert.match(player.state.error, /C command failed/);
  await env.poll(snapshot());
  assert.equal(player.state.transport, 'error', 'an unapplied old state must not erase the latest failure');
  const retry = player.toggle(); await settle();
  selected(player, 2);
  assert.equal(commands[3].id, 'C');
  commands[3].resolve(2); await retry;
  await env.poll(snapshot(2, 2, { elapsed: 0 })); selected(player, 2, 'playing');
  await env.poll(snapshot(2, 2, { playing: false, transport: 'error', error: 'decoder failed' }));
  assert.equal(player.state.error, 'decoder failed', 'current worker errors still surface');
});

test('stop wins over pending play and stale polls, then allows replay', async t => {
  const env = await fixture(t), { player, commands } = env;
  const b = player.next(); await settle();
  player.stop(); selected(player, 1, 'idle');
  commands[1].reject(new Error('obsolete B')); await b; await settle();
  assert.equal(commands[2].operation, 'stop');
  await env.poll(snapshot()); selected(player, 1, 'idle');
  commands[2].resolve(2); await settle();
  await env.poll(snapshot()); selected(player, 1, 'idle');
  await env.poll(snapshot(1, 2, { elapsed: 0, playing: false, transport: 'idle' })); selected(player, 1, 'idle');
  const retry = player.toggle(); await settle();
  commands[3].resolve(3); await retry;
  await env.poll(snapshot(1, 3, { elapsed: 0 })); selected(player, 1, 'playing');
});

test('pause and seek keep their latest intent until applied, then resume normal progress', async t => {
  const env = await fixture(t), { player, commands } = env;
  const pause = player.toggle(); await settle();
  selected(player, 0, 'paused', 25);
  commands[1].resolve(2); await pause;
  await env.poll(snapshot()); selected(player, 0, 'paused', 25);
  await env.poll(snapshot(0, 2, { playing: false, transport: 'paused' }));
  player.seek(40); await settle(); selected(player, 0, 'paused', 40);
  commands[2].resolve(3); await settle();
  await env.poll(snapshot(0, 2, { playing: false, transport: 'paused' })); selected(player, 0, 'paused', 40);
  await env.poll(snapshot(0, 3, { elapsed: 40, playing: false, transport: 'paused' })); selected(player, 0, 'paused', 40);
  const resume = player.toggle(); await settle();
  commands[3].resolve(4); await resume;
  await env.poll(snapshot(0, 4, { elapsed: 40 })); selected(player, 0, 'playing', 40);
  await env.poll(snapshot(0, 4, { elapsed: 41 })); selected(player, 0, 'playing', 41);
});

test('worker startup failure ends loading even when the accepted command cannot be applied', async t => {
  const env = await fixture(t), { player, commands } = env;
  const b = player.next(); await settle();
  commands[1].resolve(2); await b;
  await env.poll(snapshot(0, 1, { workerError: 'Audio device unavailable', error: 'Audio device unavailable', transport: 'error', playing: false }));
  assert.equal(player.state.currentTrack.id, 'B');
  assert.equal(player.state.transport, 'error');
  assert.equal(player.state.loading, false);
  assert.equal(player.state.playing, false);
  assert.equal(player.state.error, 'Audio device unavailable');
  const retry = player.toggle(); await settle();
  commands[2].reject(new Error('audio worker disconnected')); await retry;
  assert.equal(player.state.transport, 'error');
  assert.equal(player.state.loading, false);
  assert.match(player.state.error, /disconnected/);
});

test('current poll failures surface and recover, while disposed calls remain silent', async t => {
  const env = await fixture(t), { player, commands } = env;
  await env.poll(null, new Error('current poll unavailable'));
  assert.match(player.state.error, /current poll unavailable/);
  await env.poll(snapshot()); selected(player, 0, 'playing', 25);
  let emitted = 0;
  player.subscribe(() => emitted++);
  const b = player.next(); await settle();
  const before = player.state, count = emitted;
  player.dispose();
  commands[1].reject(new Error('late disposal error')); await b;
  assert.deepEqual(player.state, before);
  assert.equal(emitted, count);
});
