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

 test('wheel order, empty slots and labels survive normalization; unsupported schemes are dropped', () => {
  const shortcut = 'shortcuts://run-shortcut?name=Test';
  const s = sanitize({ trigger: { vk: 0x56 }, wheel: ['', '/Applications/A.app', shortcut], names: { '/Applications/A.app': ' 工作 ', unknown: 'discard' } });
  assert.equal(s.wheel.length, 8);
  assert.equal(s.wheel[0], '');
  assert.equal(s.wheel[2], '');
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

test('localized software search finds Chinese names, original names and aliases without truncation', async () => {
  const { matchesApp } = await import('../src/shared/appSearch.ts');
  const catalog = Array.from({ length: 80 }, (_, i) => ({ name: `软件 ${i}`, path: `/Applications/${i}.app`, aliases: [] as string[] }));
  catalog.push({ name: '微信', path: '/Applications/WeChat.app', aliases: ['WeChat'] });
  assert.equal(catalog.filter((app) => matchesApp(app, '')).length, 81);
  assert.equal(catalog.filter((app) => matchesApp(app, '微信'))[0].name, '微信');
  assert.equal(catalog.filter((app) => matchesApp(app, '  weCHAT  '))[0].name, '微信');
  assert.equal(matchesApp({ name: '豆包浏览器', path: '/Applications/Doubao Browser.app' }, 'doubao'), true);
});

test('outer content survives save/reload and rejects executable URL schemes and stale owners', () => {
  const owner = '/Applications/Browser.app';
  const s = sanitize({ wheel: [owner], content: {
    [owner]: [{ id: 'folder', name: '文档', target: '/Users/example/Documents' }, { id: 'site', name: '文档站', target: 'https://example.com' },
      { id: 'bad', name: 'bad', target: 'javascript:alert(1)' }, { id: 'site', name: 'duplicate', target: 'https://other.test' }],
    unknown: [{ id: 'x', name: 'x', target: '/tmp' }],
  } });
  assert.equal(s.content[owner].length, 2);
  assert.equal(s.content.unknown, undefined);
  assert.deepEqual(sanitize(s), s);
});

test('configurable wheel size preserves all 24 actions and has matching slice geometry', () => {
  for (let count = 4; count <= 12; count++) {
    const wheel = Array.from({ length: 24 }, (_, i) => `/Applications/App${i}.app`);
    const s = sanitize({ slotsPerPage: count, wheel });
    assert.deepEqual(s.wheel.filter(Boolean), wheel);
    assert.equal(s.wheel.length % count, 0);
    const b = wheelBoxAt(700, 450, 0, 25, 520, count);
    for (let i = 0; i < count; i++) {
      const angle = -Math.PI / 2 + i * Math.PI * 2 / count;
      assert.equal(slotAt(b.cx + Math.cos(angle) * 120, b.cy + Math.sin(angle) * 120, b), i);
    }
  }
  const source = Array<string>(33).fill(''); source[32] = '/Applications/Keep.app';
  assert.deepEqual(sanitize({ slotsPerPage: 12, wheel: source }).wheel.filter(Boolean), ['/Applications/Keep.app']);
});

test('direction selection has a narrow deadband and still cancels in the hub', async () => {
  const { stableSlotAt } = await import('../src/shared/geometry.ts');
  const b = box(8), angle = -Math.PI / 2 + Math.PI / 8;
  assert.equal(stableSlotAt(...at(angle + Math.PI / 180), b, 0), 0);
  assert.equal(stableSlotAt(...at(angle + 5 * Math.PI / 180), b, 0), 1);
  assert.equal(stableSlotAt(0, 0, b, 0), null);
  assert.equal(stableSlotAt(OUTER_RADIUS + 1, 0, b, 0), null);
});

test('all supplied cat animation frames remain inside their original sheet', async () => {
  const { mascotFrame, MASCOT_ROWS, MASCOT_SHEET } = await import('../src/shared/mascotFrames.ts');
  for (let row = 0; row < MASCOT_ROWS.length; row++) {
    const seen = new Set<number>();
    for (let ms = 0; ms < 2000; ms += 20) {
      const frame = mascotFrame(row, ms);
      assert.ok(frame.x >= 0 && frame.y >= 0);
      assert.ok(frame.x + frame.width <= MASCOT_SHEET.width);
      assert.ok(frame.y + frame.height <= MASCOT_SHEET.height);
      seen.add(frame.frame);
    }
    assert.equal(seen.size, MASCOT_ROWS[row].frames);
    assert.equal(mascotFrame(row, 1500, 9, true).frame, 0);
  }
});

test('moving an existing action across pages swaps rather than losing the destination', async () => {
  const { applyPagePatch, dropPatch } = await import('../src/shared/wheelLayout.ts');
  const all = Array.from({ length: 16 }, (_, i) => `/Applications/App${i}.app`);
  const page = all.slice(0, 8);
  const patch = dropPatch(page, '', { from: 'slot', id: all[8], slot: 0 }, 3)!;
  const next = applyPagePatch(all, 0, 8, patch.wheel!);
  assert.equal(next[3], all[8]); assert.equal(next[8], all[3]);
  assert.equal(new Set(next).size, 16);
  assert.deepEqual([...next].sort(), [...all].sort());
});

test('all outer labels are laid out without pages and can be hit', async () => {
  const { windowChips, chipAt, contentBounds } = await import('../src/shared/windows.ts');
  for (const count of [9, 13, 24, 40]) {
    const chips = windowChips(735, 450, 2, count, 520, 8, { width: 1470, height: 956 });
    assert.equal(chips.length, count);
    for (const chip of chips) assert.equal(chipAt(chip.x + chip.width / 2, chip.y + chip.height / 2, chips), chip.index);
    const bounds = contentBounds(chips)!;
    assert.ok(bounds.x >= 0 && bounds.y >= 0 && bounds.x + bounds.width <= 1470 && bounds.y + bounds.height <= 956);
  }
});

test('preview follows software hover and keeps the real browser window identity', async () => {
  const { previewWindowFor } = await import('../src/shared/content.ts');
  const window = { id: '1:3', pid: 1, windowId: 3, title: 'Current window', appPath: '/Browser.app', rawTitle: 'Current window', index: 0, minimized: false, bounds: [] };
  const tab = { id: 'tab', kind: 'tab' as const, pid: 1, windowId: 3, title: 'Inactive tab', appPath: '/Browser.app', index: 1, selected: false };
  const owner = { name: 'Browser', path: '/Browser.app', windows: [window], tabs: [tab] };
  assert.equal(previewWindowFor(owner, null), window);
  assert.equal(previewWindowFor(owner, { kind: 'tab', id: tab.id, title: tab.title, tab }), window);
  const fallback = previewWindowFor({ ...owner, windows: [] }, { kind: 'tab', id: tab.id, title: tab.title, tab })!;
  assert.equal(fallback.windowId, 3); assert.equal(fallback.pid, 1);
  assert.notEqual(fallback.title, tab.title);
  assert.equal(previewWindowFor(undefined, null), null);
});

test('large label lists stay out of the central cancel hub', async () => {
  const { windowChips, chipAt } = await import('../src/shared/windows.ts');
  for (const cx of [30, 735, 1440]) for (let slot = 0; slot < 8; slot++) for (const count of [9, 24, 36, 60]) {
    const chips = windowChips(cx, 450, slot, count, 520, 8, { width: 1470, height: 956 });
    assert.equal(chips.length, count);
    for (const chip of chips) {
      const nearestX = Math.max(chip.x, Math.min(cx, chip.x + chip.width));
      const nearestY = Math.max(chip.y, Math.min(450, chip.y + chip.height));
      assert.ok(Math.hypot(nearestX - cx, nearestY - 450) > 86 * 520 / 600);
      assert.equal(chipAt(chip.x + chip.width / 2, chip.y + chip.height / 2, chips), chip.index);
    }
  }
});
