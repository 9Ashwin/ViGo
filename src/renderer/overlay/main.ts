import { Wheel } from './wheel';
import type { OverlayApi } from '../../preload/overlay';
import type { OpenPayload } from '../../main/overlay';
import { WHEEL } from '../../shared/geometry';
import mascot from '../assets/mascot.png';

const api = (window as unknown as { vigo: OverlayApi }).vigo;
const wheel = new Wheel(document.getElementById('wheel') as unknown as SVGSVGElement, '', mascot, () => undefined);
let lastPayload: OpenPayload | null = null;
function clearTransient(): void {
  (document.getElementById('window-chips') as HTMLDivElement).replaceChildren();
  (document.getElementById('window-preview') as HTMLDivElement).hidden = true;
}
function renderPayload(p: OpenPayload, open: boolean): void {
  if (open) { clearTransient(); (document.getElementById('action-message') as HTMLDivElement).hidden = true; }
  lastPayload = p;
  wheel.setItems(p.items.map((item) => ({ key: item.path, label: item.name, icon: item.icon })));
  wheel.setSize(p.size);
  if (open) wheel.open(p.cx, p.cy);
  const indicator = document.getElementById('page-indicator') as HTMLDivElement;
  indicator.style.left = `${p.cx}px`;
  indicator.style.top = `${p.cy + p.size / 2 + 10}px`;
  indicator.textContent = [0, 1, 2].map((n) => n === p.page ? '●' : '○').join('  ');
  indicator.hidden = false;
  const running = document.getElementById('running-indicators') as HTMLDivElement;
  running.replaceChildren(...p.items.flatMap((item, i) => {
    if (!item.running) return [];
    const angle = -Math.PI / 2 + i * Math.PI / 4;
    const dot = document.createElement('i');
    dot.style.left = `${p.cx + Math.cos(angle) * WHEEL.iconR * p.size / 600}px`;
    dot.style.top = `${p.cy + Math.sin(angle) * WHEEL.iconR * p.size / 600 + 25 * p.size / 600}px`;
    return [dot];
  }));
  if (open) (document.getElementById('window-preview') as HTMLDivElement).hidden = true;
}
api.onOpen((p) => renderPayload(p, true));
api.onUpdate((p) => renderPayload(p, false));
api.onHover((i) => wheel.hover(i));
api.onClose(() => { clearTransient(); wheel.cancel(); (document.getElementById('page-indicator') as HTMLDivElement).hidden = true; });

api.onWindows(({ windows, chips }) => {
  const host = document.getElementById('window-chips') as HTMLDivElement;
  host.replaceChildren(...chips.map((chip) => {
    const el = document.createElement('div');
    el.className = 'window-chip';
    el.style.cssText = `left:${chip.x}px;top:${chip.y}px;width:${chip.width}px;height:${chip.height}px`;
    el.textContent = `${windows[chip.index].minimized ? '◇ ' : '▱ '}${windows[chip.index].title}`;
    return el;
  }));
});
api.onWindowHover((index) => document.querySelectorAll('.window-chip').forEach((el, i) => el.classList.toggle('selected', index === i)));
api.onPreview((payload) => {
  const host = document.getElementById('window-preview') as HTMLDivElement;
  host.hidden = !payload;
  if (!payload) return;
  host.replaceChildren();
  if (lastPayload) {
    const edge = lastPayload.size * .43 + 75;
    const right = lastPayload.cx + edge + 12;
    const left = lastPayload.cx - edge - 252;
    const x = right + 240 <= innerWidth ? right : left >= 8 ? left : Math.max(8, Math.min(innerWidth - 248, right));
    host.style.left = `${x}px`;
    host.style.right = 'auto';
    host.style.top = `${Math.max(8, Math.min(innerHeight - 190, lastPayload.cy - 90))}px`;
    host.style.bottom = 'auto';
  }
  if (payload.image) { const img = document.createElement('img'); img.src = payload.image; host.append(img); }
  const label = document.createElement('span');
  label.textContent = `${payload.title}${payload.cached ? ' · 上次画面' : ''}`;
  host.append(label);
});

api.onMessage((message) => {
  const el = document.getElementById('action-message') as HTMLDivElement;
  el.textContent = message;
  el.hidden = !message;
});
