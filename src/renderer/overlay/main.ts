import { SpriteMascot } from '../shared/mascot';
import { Wheel } from './wheel';
import type { OverlayApi } from '../../preload/overlay';
import type { OpenPayload } from '../../main/overlay';
import { contentBounds } from '../../shared/windows';
import { WHEEL } from '../../shared/geometry';

const api = (window as unknown as { vigo: OverlayApi }).vigo;
const wheel = new Wheel(document.getElementById('wheel') as unknown as SVGSVGElement, '', () => undefined);
let errorMascot: SpriteMascot | null = null;
let currentOwner: number | null = null;
let lastPayload: OpenPayload | null = null;
let displayedPreview: string | null = null;
let previewGeneration = 0;
let imageSequence = 0;
let lastImage = '';
let pendingImage = '';
function clearTransient(): void {
  displayedPreview = null; previewGeneration++; imageSequence++; lastImage = ''; pendingImage = '';
  (document.getElementById('window-chips') as HTMLDivElement).replaceChildren();
  (document.getElementById('window-preview') as HTMLDivElement).hidden = true;
}
function renderPayload(p: OpenPayload, open: boolean): void {
  if (open) { clearTransient(); errorMascot?.stop(); errorMascot = null; (document.getElementById('action-message') as HTMLDivElement).hidden = true; }
  lastPayload = p;
  wheel.setItems(p.items.map((item) => ({ key: item.path, label: item.name, icon: item.icon })));
  wheel.setSize(p.size);
  if (open) wheel.open(p.cx, p.cy);
  const indicator = document.getElementById('page-indicator') as HTMLDivElement;
  indicator.style.left = `${p.cx}px`;
  indicator.style.top = `${p.cy + p.size * .34}px`;
  indicator.textContent = Array.from({ length: p.pages }, (_, n) => n).map((n) => n === p.page ? '●' : '○').join('  ');
  indicator.hidden = false;
  const running = document.getElementById('running-indicators') as HTMLDivElement;
  running.replaceChildren(...p.items.flatMap((item, i) => {
    if (!item.running) return [];
    const angle = -Math.PI / 2 + i * Math.PI * 2 / p.items.length;
    const dot = document.createElement('i');
    dot.style.left = `${p.cx + Math.cos(angle) * WHEEL.iconR * p.size / 600}px`;
    dot.style.top = `${p.cy + Math.sin(angle) * WHEEL.iconR * p.size / 600 + 25 * p.size / 600}px`;
    return [dot];
  }));
  if (open) (document.getElementById('window-preview') as HTMLDivElement).hidden = true;
}
api.onOpen((p) => renderPayload(p, true));
api.onUpdate((p) => renderPayload(p, false));
api.onPointer((p) => wheel.look(p.dx, p.dy, p.hub));
api.onTheme((dark) => { document.documentElement.dataset.theme = dark ? 'dark' : 'light'; });
api.onHover((i) => { currentOwner = i; wheel.hover(i); });
api.onClose(() => { clearTransient(); wheel.cancel(); (document.getElementById('page-indicator') as HTMLDivElement).hidden = true; });

api.onWindows(({ entries, chips }) => {
  const host = document.getElementById('window-chips') as HTMLDivElement;
  host.replaceChildren();
  if (!chips.length) { if (lastPayload) (document.getElementById('page-indicator') as HTMLDivElement).hidden = false; return; }
  (document.getElementById('page-indicator') as HTMLDivElement).hidden = true;
  const panels = new Map<number, typeof chips>();
  chips.forEach((chip) => { const id = chip.panel ?? 0, list = panels.get(id) ?? []; list.push(chip); panels.set(id, list); });
  for (const panel of panels.values()) {
  const bounds = contentBounds(panel)!; const { x, y } = bounds;
  const group = document.createElement('div'); group.className = 'content-popover';
  group.style.cssText = `left:${x}px;top:${y}px;width:${bounds.width}px;height:${bounds.height}px`;
  panel.forEach((chip) => {
    const item = entries[chip.index], el = document.createElement('div'); el.className = 'window-chip'; el.dataset.index = String(chip.index);
    el.style.cssText = `left:${chip.x - x}px;top:${chip.y - y}px;width:${chip.width}px;height:${chip.height}px`;
    const owner = currentOwner === null ? null : lastPayload?.items[currentOwner];
    if (owner?.icon) { const image = document.createElement('img'); image.className = 'chip-app-icon'; image.src = owner.icon; image.alt = ''; el.append(image); }
    const label = document.createElement('span'); label.className = 'chip-title'; label.textContent = item.title; el.append(label);
    if (item.kind === 'tab' && item.tab.selected) { const current = document.createElement('small'); current.textContent = '当前'; el.append(current); }
    group.append(el);
  });
  host.append(group);
  }
});
api.onWindowHover((index) => document.querySelectorAll('.window-chip').forEach((el) => el.classList.toggle('selected', index === Number((el as HTMLElement).dataset.index))));
api.onPreview((payload) => {
  const host = document.getElementById('window-preview') as HTMLDivElement;
  host.hidden = !payload;
  if (!payload) { displayedPreview = null; previewGeneration++; imageSequence++; lastImage = ''; pendingImage = ''; return; }
  if (displayedPreview !== payload.id) {
    displayedPreview = payload.id; previewGeneration++; imageSequence++; lastImage = ''; pendingImage = '';
    host.innerHTML = '<div class="preview-media"><img class="preview-picture" hidden alt="窗口画面"/><div class="preview-placeholder"><i></i><span></span></div></div><div class="preview-caption"><span class="preview-title"></span><small class="preview-status"></small></div>';
  }
  host.style.left = `${payload.x}px`; host.style.top = `${payload.y}px`;
  host.style.right = 'auto'; host.style.bottom = 'auto';
  host.dataset.state = payload.state;
  (host.querySelector('.preview-title') as HTMLElement).textContent = payload.title;
  (host.querySelector('.preview-status') as HTMLElement).textContent = payload.state === 'live' ? '窗口 · 实时' : payload.cached ? '窗口 · 缓存' : '';
  const placeholder = host.querySelector('.preview-placeholder') as HTMLElement;
  (placeholder.querySelector('span') as HTMLElement).textContent = payload.message || (payload.state === 'unavailable' ? '这个窗口暂时无法预览' : '正在载入画面…');
  placeholder.hidden = Boolean(lastImage);
  if (payload.image && payload.image !== lastImage && payload.image !== pendingImage) {
    pendingImage = payload.image;
    const generation = previewGeneration, sequence = ++imageSequence, image = new Image(); image.src = payload.image;
    void image.decode().then(() => {
      if (generation !== previewGeneration || sequence !== imageSequence) return;
      const target = host.querySelector('.preview-picture') as HTMLImageElement;
      target.src = payload.image; target.hidden = false; placeholder.hidden = true; lastImage = payload.image; pendingImage = '';
    }).catch(() => { if (generation === previewGeneration && sequence === imageSequence) { pendingImage = ''; if (!lastImage) { host.dataset.state = 'unavailable'; (placeholder.querySelector('span') as HTMLElement).textContent = '画面暂时无法读取'; } } });
  }
});

api.onMessage((message) => {
  const el = document.getElementById('action-message') as HTMLDivElement;
  errorMascot?.stop(); errorMascot = null; el.replaceChildren();
  if (message) {
    const sprite = document.createElement('canvas'); sprite.classList.add('message-mascot');
    const text = document.createElement('span'); text.textContent = message; el.append(sprite, text);
    errorMascot = new SpriteMascot(sprite, 58, 64); errorMascot.emote(8); errorMascot.start();
  }
  el.hidden = !message;
});
