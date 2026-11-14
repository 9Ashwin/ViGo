// Unit tests for the pure parts: the ring's geometry (which the canvas and the hit test share), the helper
// protocol and the settings sanitiser. Run: npm test
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { INNER_RADIUS, OUTER_RADIUS, slotAngles, slotAt, wheelBoxAt, type WheelBox } from '../src/shared/geometry.ts';
import { parseHelperLine, serializeCommand } from '../src/shared/protocol.ts';
import { sanitize } from '../src/shared/settings.ts';
import { AccessibilityGate } from '../src/main/accessibilityGate.ts';

const box = (count: number): WheelBox => ({ cx: 0, cy: 0, count, inner: INNER_RADIUS, outer: OUTER_RADIUS });

/** A point on the ring's midline at `angle` (radians, 0 = right, growing clockwise). */
const at = (angle: number): [number, number] => {
  const r = (INNER_RADIUS + OUTER_RADIUS) / 2;
  return [Math.cos(angle) * r, Math.sin(angle) * r];
};

test('slice 0 sits at the top and the ring runs clockwise', () => {
  const b = box(4);
  const top = at(-Math.PI / 2);
  assert.equal(slotAt(top[0], top[1], b), 0);
  const right = at(0);
  assert.equal(slotAt(right[0], right[1], b), 1);
  const bottom = at(Math.PI / 2);
  assert.equal(slotAt(bottom[0], bottom[1], b), 2);
  const left = at(Math.PI);
  assert.equal(slotAt(left[0], left[1], b), 3);
});

test('the hub and everything past the rim are no slice at all', () => {
  const b = box(8);
  assert.equal(slotAt(0, 0, b), null);
  assert.equal(slotAt(INNER_RADIUS - 1, 0, b), null);
  assert.equal(slotAt(OUTER_RADIUS + 1, 0, b), null);
  assert.equal(slotAt(INNER_RADIUS + 1, 0, b), 2); // just off the hub, pointing right: slice 2 of 8
});

test('an empty ring has no slices, and north stays in slice zero', () => {
  assert.equal(slotAt(0, -(INNER_RADIUS + OUTER_RADIUS) / 2, box(0)), null);
  const b = box(3);
  // The north-facing sector spans both sides of the vertical axis.
  const [x, y] = at(-Math.PI / 2 - 0.001);
  assert.equal(slotAt(x, y, b), 0);
});

test('slice angles tile the circle without gaps', () => {
  for (const count of [1, 3, 8, 12]) {
    const total = Array.from({ length: count }, (_, i) => {
      const { start, end } = slotAngles(i, count);
      assert.ok(end > start);
      return end - start;
    }).reduce((a, b) => a + b, 0);
    assert.ok(Math.abs(total - Math.PI * 2) < 1e-9);
  }
});

test('the overlay hit test accounts for a nonzero window-local centre', () => {
  const b = { cx: 380, cy: 380, count: 8, inner: 74, outer: 249 };
  assert.equal(slotAt(500, 380, b), 2);
  assert.equal(slotAt(380, 260, b), 0);
  assert.equal(slotAt(380, 380, b), null);
  assert.equal(slotAt(700, 380, b), null);
});

test('releasing without movement at a clamped display edge remains a cancellation', () => {
  const b = wheelBoxAt(700, 180, 320, 33, 520);
  assert.equal(slotAt(700 - 320, 180 - 33, b), null);
  assert.equal(slotAt(820 - 320, 180 - 33, b), 2);
});

test('helper lines parse only what the protocol defines', () => {
  assert.deepEqual(parseHelperLine('{"type":"ready","version":6}'), { type: 'ready', version: 6 });
  assert.deepEqual(parseHelperLine('{"type":"wheelOpen","x":10,"y":20,"wheel":"ping"}'), {
    type: 'wheelOpen', x: 10, y: 20, wheel: 'ping',
  });
  assert.deepEqual(parseHelperLine('{"type":"cancel"}'), { type: 'cancel' });
  assert.equal(parseHelperLine('{"type":"wheelOpen","x":1.5,"y":2}'), null);
  assert.equal(parseHelperLine('not json'), null);
  assert.equal(parseHelperLine('{"type":"mystery"}'), null);
});

test('commands serialize as one JSON object per line', () => {
  const line = serializeCommand({ type: 'setEnabled', enabled: false });
  assert.equal(line, '{"type":"setEnabled","enabled":false}\n');
  assert.equal(line.split('\n').filter(Boolean).length, 1);
});

test('settings drop junk and keep what they understand', () => {
  const s = sanitize({
    trigger: 'mouse4',
    wheel: ['/Applications/A.app', '/Applications/A.app', 'relative/path', 42],
    launchAtLogin: true,
    junk: 'ignored',
  });
  assert.equal(s.trigger, 'mouse4');
  assert.deepEqual(s.wheel.filter(Boolean), ['/Applications/A.app']);
  assert.equal(s.launchAtLogin, true);

  const fallback = sanitize({ trigger: 'nope', apps: 'not an array' });
  assert.deepEqual(fallback.trigger, { vk: 0x56 });
  assert.deepEqual(fallback.wheel, Array(8).fill(''));
});

test('the ring cannot hold more apps than it has comfortable slices', () => {
  const many = Array.from({ length: 30 }, (_, i) => `/Applications/App${i}.app`);
  assert.equal(sanitize({ wheel: many }).wheel.filter(Boolean).length, 24);
});

test('permission denial waits silently and a grant starts the helper once', () => {
  const gate = new AccessibilityGate();
  for (let i = 0; i < 20; i += 1) assert.deepEqual(gate.observe(false, 'starting'), {
    status: 'noAccess', stale: false, action: null,
  });
  assert.equal(gate.observe(true, 'starting').action, 'start');
  assert.equal(gate.observe(true, 'running').status, 'running');
  assert.equal(gate.observe(true, 'running').action, null);
});

test('a stale grant never loops retries, and revoking then granting recovers', () => {
  const gate = new AccessibilityGate();
  gate.observe(true, 'starting');
  for (let i = 0; i < 20; i += 1) assert.deepEqual(gate.observe(true, 'noAccess'), {
    status: 'noAccess', stale: true, action: null,
  });
  gate.retry();
  assert.equal(gate.observe(true, 'noAccess').action, 'start');
  assert.equal(gate.observe(false, 'running').action, 'stop');
  assert.equal(gate.observe(false, 'running').action, null);
  assert.equal(gate.observe(true, 'noAccess').action, 'start');
});

 test('wheel order, empty slots, labels, and shortcut targets survive normalization', () => {
  const shortcut = 'shortcuts://run-shortcut?name=Test';
  const s = sanitize({ trigger: { vk: 0x56 }, wheel: ['', '/Applications/A.app', shortcut], names: { '/Applications/A.app': ' 工作 ', unknown: 'discard' } });
  assert.equal(s.wheel.length, 8);
  assert.equal(s.wheel[0], '');
  assert.equal(s.wheel[2], shortcut);
  assert.deepEqual(s.names, { '/Applications/A.app': '工作' });
  assert.deepEqual(sanitize(s), s);
});

test('forced helper shutdown waits for exit before retry can start another process', async (t) => {
  const { EventEmitter } = await import('node:events');
  const { PassThrough } = await import('node:stream');
  const { InputBridge } = await import('../src/main/inputBridge.ts');
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const children: Array<InstanceType<typeof EventEmitter> & { stdin: InstanceType<typeof PassThrough>; stdout: InstanceType<typeof PassThrough>; stderr: InstanceType<typeof PassThrough>; exitCode: null; signalCode: null; kill: (signal?: string) => boolean }> = [];
  let kills = 0;
  const fakeSpawn = () => {
    const child = Object.assign(new EventEmitter(), { stdin: new PassThrough(), stdout: new PassThrough(), stderr: new PassThrough(), exitCode: null, signalCode: null,
      kill: () => { kills++; return true; } });
    children.push(child);
    return child;
  };
  const bridge = new InputBridge({ command: 'unused', spawn: fakeSpawn as never });
  bridge.start();
  let stopped = false;
  const stop = bridge.stop().then(() => { stopped = true; bridge.retry(); });
  t.mock.timers.tick(1000);
  await Promise.resolve();
  assert.equal(kills, 1);
  assert.equal(stopped, false);
  assert.equal(children.length, 1);
  children[0].emit('exit', 0);
  await stop;
  assert.equal(children.length, 2);
  children[0].stdout.write('{"type":"ready","version":6}\n');
  assert.equal(bridge.status, 'starting');
  children[1].stdout.write('{"type":"ready","version":6}\n');
  assert.equal(bridge.status, 'running');
  const finalStop = bridge.stop();
  children[1].emit('exit', 0);
  await finalStop;
});

test('window capsules stay separate and their displayed rectangles select the same window', async () => {
  const { windowChips, chipAt } = await import('../src/shared/windows.ts');
  for (let slot = 0; slot < 8; slot++) for (let count = 1; count <= 12; count++) {
    const chips = windowChips(380, 380, slot, count, 520);
    assert.equal(chips.length, count);
    for (const chip of chips) assert.equal(chipAt(chip.x + chip.width / 2, chip.y + chip.height / 2, chips), chip.index);
  }
});
