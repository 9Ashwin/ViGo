// Exercise the actual saved bookmark, including off-site and malformed-page rejection.
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const html = readFileSync(new URL('../../assets/companion-browser.html', import.meta.url), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const nodes = new Map();
const element = id => {
  if (!nodes.has(id)) nodes.set(id, { setAttribute(k, v) { this[k] = v; }, addEventListener() {} });
  return nodes.get(id);
};
const setup = vm.createContext({ URL, document: { querySelector: element } });
vm.runInContext(script, setup);
const saved = element('#bookmark').href;
assert(saved.startsWith('javascript:'));
assert.equal(element('#code').value, saved);
for (const [url, expected] of [
  ['https://codex-pets.net/#/pets/pet-6af8bc5f4c0c81918e60784c437b7a4a', 'pet-6af8bc5f4c0c81918e60784c437b7a4a'],
  ['https://codex-pets.net/pets/aegis-sentinel', 'aegis-sentinel'],
  ['https://codex-pets.net/#/pets/Pet_42?view=preview', 'Pet_42'],
  ['https://codex-pets.net/', null],
  ['https://codex-pets.net/#/collections/cats', null],
  ['https://codex-pets.net.evil.test/#/pets/cat', null],
  ['https://codex-pets.net@evil.test/#/pets/cat', null],
  ['http://codex-pets.net/#/pets/cat', null],
  ['https://codex-pets.net:8443/#/pets/cat', null],
  ['file:///tmp/page.html#/pets/cat', null],
  ['https://codex-pets.net/#/pets/%2e%2e', null],
  ['https://codex-pets.net/#/pets/' + 'a'.repeat(97), null],
]) {
  const alerts = [];
  const context = vm.createContext({ URL, location: { href: url }, alert: value => alerts.push(value) });
  vm.runInContext(saved.slice('javascript:'.length), context);
  if (expected) {
    assert.equal(context.location.href, 'vigo://companion/import/' + expected, url);
    assert.equal(alerts.length, 0);
  } else {
    assert.equal(context.location.href, url, url);
    assert.equal(alerts.length, 1, url);
  }
}
assert(html.includes('href="vigo://companion/import/pet-6af8bc5f4c0c81918e60784c437b7a4a"'));
console.log('Browser bookmark: valid pet routes, launch link, and rejected origins/paths verified.');
