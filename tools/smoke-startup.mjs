// Real Electron startup regression: ready -> settings IPC -> native icons -> painted wheel -> clean quit.
// Build first. Optional .app argument tests a packaged app through LaunchServices.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { createServer } from 'node:net';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const require = createRequire(import.meta.url);
const server = createServer();
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const port = server.address().port;
await new Promise((done) => server.close(done));
const bundle = process.argv[2] && resolve(process.argv[2]);
const args = [`--inspect=127.0.0.1:${port}`];
const child = bundle
  ? spawn('/usr/bin/open', ['-n', bundle, '--args', ...args], { stdio: 'inherit' })
  : spawn(require('electron'), ['.', ...args], { stdio: 'inherit' });
let socket;
let pid;
let sequence = 0;
const pending = new Map();
const deadline = Date.now() + 15_000;
function stopOwnedProcess() {
  try {
    if (pid) process.kill(pid, 'SIGTERM');
    else if (!bundle) child.kill();
  } catch (error) {
    if (error.code !== 'ESRCH') throw error;
  }
}
const watchdog = setTimeout(() => {
  stopOwnedProcess();
  console.error('Startup smoke timed out');
  process.exit(1);
}, 25_000);

async function poll(read, accept) {
  while (Date.now() < deadline) {
    const result = await read();
    if (accept(result)) return result;
    await delay(100);
  }
  throw new Error('Startup did not reach the expected state within 15 seconds');
}

async function request(method, params) {
  const id = ++sequence;
  const result = new Promise((done, fail) => pending.set(id, { done, fail }));
  socket.send(JSON.stringify({ id, method, params }));
  const reply = await result;
  if (reply.error || reply.result.exceptionDetails) throw new Error(JSON.stringify(reply));
  return reply.result;
}

async function evaluate(expression) {
  return (await request('Runtime.evaluate', {
    expression, returnByValue: true, awaitPromise: true,
  })).result.value;
}

try {
  const targets = await poll(
    () => fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json()).catch(() => []),
    (list) => list.length > 0,
  );
  socket = new WebSocket(targets[0].webSocketDebuggerUrl);
  await new Promise((done, fail) => { socket.onopen = done; socket.onerror = fail; });
  socket.onmessage = ({ data }) => {
    const reply = JSON.parse(data);
    pending.get(reply.id)?.done(reply);
    pending.delete(reply.id);
  };
  socket.onclose = () => {
    for (const { fail } of pending.values()) fail(new Error('Electron inspector disconnected'));
    pending.clear();
  };
  const electron = "process.mainModule.require('electron')";
  // The inspector may first expose Node's temporary bootstrap context, which is then discarded.
  await poll(() => evaluate("typeof process !== 'undefined' && !!process.mainModule && typeof setTimeout === 'function'")
    .catch(() => false), Boolean);
  pid = await evaluate('process.pid');
  await poll(() => evaluate(`${electron}.app.isReady()`), Boolean);
  if (await evaluate(`${electron}.powerMonitor.getSystemIdleState(60) === 'locked'`)) {
    const error = new Error('Desktop is locked'); error.code = 'LOCKED'; throw error;
  }
  await poll(() => evaluate(`${electron}.BrowserWindow.getAllWindows().some(w => !w.webContents.isLoading())`), Boolean);
  await evaluate(`${electron}.app.emit('second-instance')`);
  const settingsWindow = `${electron}.BrowserWindow.getAllWindows().find(w => w.webContents.getURL().includes('/settings/'))`;
  await poll(() => evaluate(`!!(${settingsWindow}) && !(${settingsWindow}).webContents.isLoading()`), Boolean);
  const state = await poll(
    () => evaluate(`(${settingsWindow}).webContents.executeJavaScript('window.vigo.get()')`),
    (s) => s.status === 'running' || s.status === 'noAccess',
  );
  assert.ok(state.items.length > 0, 'The wheel must have at least one installed app');
  const icons = await evaluate(`(${settingsWindow}).webContents.executeJavaScript(
    'Promise.all(' + ${JSON.stringify(JSON.stringify(state.items.filter((item) => item.path).map((item) => item.path)))} +
    '.map(async p => (await window.vigo.icon(p)).startsWith("data:image/png;base64,")))')`);
  assert.ok(icons.some(Boolean), 'At least one real application icon must decode');
  await evaluate(`(${settingsWindow}).webContents.executeJavaScript('window.vigo.previewWheel()')`);
  await delay(200);
  const painted = await evaluate(`(async () => {
    const w = ${electron}.BrowserWindow.getAllWindows().find(w => w.webContents.getURL().includes('/overlay/'));
    if (!w.isVisible()) return 0;
    const img = await w.webContents.capturePage();
    const bytes = img.toBitmap();
    let colored = 0;
    for (let i = 0; i < bytes.length; i += 4) if (bytes[i + 3] > 20 && Math.max(bytes[i], bytes[i + 1], bytes[i + 2]) - Math.min(bytes[i], bytes[i + 1], bytes[i + 2]) > 25) colored++;
    return colored;
  })()`);
  assert.ok(painted > 1000, 'Preview must show a painted wheel, not an empty window');
  console.log(JSON.stringify({ mode: bundle ? 'packaged' : 'development', ready: true,
    helper: state.status, trigger: state.settings.trigger, items: state.items.filter((item) => item.path).length,
    icons: icons.filter(Boolean).length, paintedPixels: painted }));
} catch (error) {
  if (error.code !== 'LOCKED') throw error;
  console.log(JSON.stringify({ mode: bundle ? 'packaged' : 'development', status: 'blocked', reason: 'desktop-locked' }));
  process.exitCode = 2;
} finally {
  if (socket?.readyState === WebSocket.OPEN) {
    try {
      await evaluate(`setTimeout(() => process.mainModule.require('electron').app.quit(), 50); true`);
    } catch {
      stopOwnedProcess();
    } finally {
      socket.close();
    }
  }
  if (pid) {
    await poll(async () => {
      try { process.kill(pid, 0); return false; } catch { return true; }
    }, Boolean);
  } else if (!bundle) child.kill();
  clearTimeout(watchdog);
}
