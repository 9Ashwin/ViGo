import { SpriteMascot } from '../shared/mascot';
import { WHEEL, pickSlice } from '../../shared/geometry';

const NS = 'http://www.w3.org/2000/svg';

function el<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number>, parent: Element): SVGElementTagNameMap[K] {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  parent.append(e);
  return e;
}

const polar = (r: number, deg: number): [number, number] => [r * Math.cos((deg * Math.PI) / 180), r * Math.sin((deg * Math.PI) / 180)];

function sector(r0: number, r1: number, a0: number, a1: number): string {
  const [x0, y0] = polar(r0, a0);
  const [x1, y1] = polar(r1, a0);
  const [x2, y2] = polar(r1, a1);
  const [x3, y3] = polar(r0, a1);
  return `M${x0},${y0} L${x1},${y1} A${r1},${r1} 0 0 1 ${x2},${y2} L${x3},${y3} A${r0},${r0} 0 0 0 ${x0},${y0}Z`;
}

/** One slice: `key` is what release() hands back, `icon` the image URL drawn in the slice, `label` shown in the centre on hover. */
export interface WheelItem {
  key: string;
  icon: string;
  label?: string;
}

/** Longer labels are cut so they stay inside the wheel's centre. */
const CENTER_LABEL_MAX = 14;
const fitLabel = (text: string): string => {
  const chars = [...text];
  return chars.length > CENTER_LABEL_MAX ? `${chars.slice(0, CENTER_LABEL_MAX - 1).join('')}…` : text;
};

/**
 * The League-style wheel (see prototype/wheel-demo.html), for pings or emotes: the slices are whatever items it is given.
 * Each wheel draws into its own svg, whose id keeps its gradient ids apart from the other wheel's.
 */
export class Wheel {
  private readonly wedges: SVGPathElement[] = [];
  private readonly plates: SVGRectElement[] = [];
  private readonly icons: SVGImageElement[] = [];
  private mascot: SpriteMascot | null = null;
  private rig: SVGGElement | null = null;
  private lookX = 0;
  private lookY = 0;
  private targetX = 0;
  private targetY = 0;
  private frame = 0;
  private hubHot = false;
  private petZoom = 1;
  private petLift = 0;
  private reduced = matchMedia('(prefers-reduced-motion: reduce)');

  look(dx: number, dy: number, hub: boolean): void {
    const length = Math.max(90, Math.hypot(dx, dy));
    this.targetX = dx / length;
    this.targetY = dy / length;
    this.hubHot = hub;
    this.mascot?.point(dx, dy, hub);
    this.svg.classList.toggle('hub-hot', hub);
    if (this.hot < 0 && this.centerTxt) this.centerTxt.textContent = hub ? '松开取消' : '';
    if (!this.frame && this.opened) this.frame = requestAnimationFrame(() => this.animateMascot());
  }

  private animateMascot(): void {
    this.frame = 0;
    if (!this.opened) return;
    const reduced = this.reduced.matches;
    this.lookX += (this.targetX - this.lookX) * (reduced ? 1 : .24);
    this.lookY += (this.targetY - this.lookY) * (reduced ? 1 : .24);
    const targetZoom = this.hubHot && !reduced ? 1.055 : 1, targetLift = this.hubHot && !reduced ? 2 : 0;
    this.petZoom += (targetZoom - this.petZoom) * .24;
    this.petLift += (targetLift - this.petLift) * .24;
    if (this.rig) this.rig.setAttribute('transform', reduced ? '' : `translate(${this.lookX * 1.8} ${this.lookY * 1.3 - this.petLift}) rotate(${this.lookX * 2.5} 0 -20) scale(${this.petZoom})`);
    if (Math.abs(this.targetX - this.lookX) + Math.abs(this.targetY - this.lookY) + Math.abs(targetZoom - this.petZoom) + Math.abs(targetLift - this.petLift) > .002) this.frame = requestAnimationFrame(() => this.animateMascot());
  }
  private centerTxt: SVGTextElement | null = null;
  /** Replaces the constructor's centre text while set. */
  private idleLabel: string | null = null;
  private dimmed = new Set<string>();
  private items: readonly WheelItem[] = [];
  private hot = -1;
  private ox = 0;
  private oy = 0;
  private opened = false;
  private scale = 1;

  constructor(
    private readonly svg: SVGSVGElement,
    private readonly centerLabel: string,
    private readonly onHover: (slice: number) => void,
  ) {
    this.build();
  }

  /** Changes what each slice holds, top (N) then clockwise. */
  setItems(items: readonly WheelItem[]): void {
    if (items.length !== this.icons.length) {
      this.svg.replaceChildren(); this.wedges.length = 0; this.icons.length = 0; this.plates.length = 0; this.mascot?.stop(); this.hot = -1;
      this.items = items; this.build();
    }
    this.items = items;
    this.icons.forEach((icon, i) => {
      const item = items[i];
      if (item?.icon) icon.setAttribute('href', item.icon);
      else icon.removeAttribute('href');
    });
    this.setDimmed([...this.dimmed]);
    this.showCenterText();
  }

  /** The centre's text while no labelled slice is hovered; null puts the wheel's own back. */
  setCenterLabel(text: string | null): void {
    this.idleLabel = text;
    this.showCenterText();
  }

  /** Greys out the slices holding these keys (an ask wheel's switched-off intents). */
  setDimmed(keys: string[]): void {
    this.dimmed = new Set(keys);
    this.icons.forEach((icon, i) => {
      const item = this.items[i];
      if (item && this.dimmed.has(item.key)) icon.classList.add('dim');
      else icon.classList.remove('dim');
    });
  }

  /** Scales the SVG and its dead zone together, keeping the press point at the centre. */
  setSize(sizePx: number): void {
    this.scale = sizePx / WHEEL.size;
    this.svg.style.width = `${sizePx}px`;
    this.svg.style.height = `${sizePx}px`;
    this.svg.style.marginLeft = `${-sizePx / 2}px`;
    this.svg.style.marginTop = `${-sizePx / 2}px`;
  }

  get isOpen(): boolean {
    return this.opened;
  }

  open(x: number, y: number): void {
    this.ox = x;
    this.oy = y;
    this.svg.style.left = `${x}px`;
    this.svg.style.top = `${y}px`;
    this.setHot(-1);
    this.opened = true;
    this.svg.classList.add('open');
    this.mascot?.start();
    this.look(0, 0, true);
  }

  move(x: number, y: number): void {
    if (this.opened) this.setHot(pickSlice(x - this.ox, y - this.oy, WHEEL.innerR * this.scale, this.items.length || 8));
  }

  /** Closes the wheel. Returns the chosen item's key at the wheel centre, or null when released in the dead zone. */
  release(x: number, y: number): { key: string; x: number; y: number } | null {
    if (!this.opened) return null;
    const slice = pickSlice(x - this.ox, y - this.oy, WHEEL.innerR * this.scale, this.items.length || 8);
    this.close();
    const item = slice >= 0 ? this.items[slice] : undefined;
    return item ? { key: item.key, x: this.ox, y: this.oy } : null;
  }

  cancel(): void {
    if (this.opened) this.close();
  }

  hover(index: number | null): void { this.setHot(index !== null && this.items[index]?.key ? index : -1); }

  private close(): void {
    this.opened = false;
    cancelAnimationFrame(this.frame); this.frame = 0;
    this.mascot?.stop();
    this.svg.classList.remove('open');
    this.setHot(-1);
  }

  private setHot(i: number): void {
    if (i === this.hot) return;
    if (this.hot >= 0) {
      this.wedges[this.hot].setAttribute('fill-opacity', '0');
      this.icons[this.hot].classList.remove('hot');
      this.plates[this.hot]?.setAttribute('fill-opacity', '0');
    }
    this.hot = i;
    if (i >= 0) {
      this.wedges[i].setAttribute('fill-opacity', '0.18');
      this.icons[i].classList.add('hot');
      this.plates[i]?.setAttribute('fill-opacity', '.2');
      if (this.opened) this.onHover(i);
    }
    this.showCenterText();
  }

  private showCenterText(): void {
    if (!this.centerTxt) return;
    const label = this.hot >= 0 ? this.items[this.hot]?.label : undefined;
    this.centerTxt.textContent = label ? fitLabel(label) : this.idleLabel ?? (this.hubHot && this.opened ? '松开取消' : this.centerLabel);
  }

  private build(): void {
    const { innerR, outerR, extR, iconR, iconSize } = WHEEL;
    const svg = this.svg;
    const id = `vigo-${svg.id}`;
    el('defs', {}, svg).innerHTML = `
      <radialGradient id="${id}-ext" cx="0" cy="0" r="${extR}" gradientUnits="userSpaceOnUse">
        <stop offset="${outerR / extR}" stop-color="#5c6066" stop-opacity=".08"/><stop offset="1" stop-color="#5c6066" stop-opacity="0"/></radialGradient>
      <radialGradient id="${id}-ring" cx="0" cy="0" r="${outerR}" gradientUnits="userSpaceOnUse">
        <stop offset="${innerR / outerR}" stop-color="#2a2d31"/><stop offset="1" stop-color="#4a4e54"/></radialGradient>
      <radialGradient id="${id}-hot" cx="0" cy="0" r="${extR}" gradientUnits="userSpaceOnUse">
        <stop offset="${innerR / extR}" stop-color="#426382"/><stop offset="${outerR / extR}" stop-color="#78a3c7"/>
        <stop offset="1" stop-color="#78a3c7" stop-opacity="0"/></radialGradient>
      <radialGradient id="${id}-core" cx="0" cy="-10" r="${innerR}" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#292b31"/><stop offset="1" stop-color="#14151a"/></radialGradient>`;

    const count = this.items.length || 8, step = 360 / count;
    const slots = [...Array(count).keys()];
    el('circle', { r: (innerR + outerR) / 2, fill: 'none', stroke: `url(#${id}-ring)`, 'stroke-width': outerR - innerR, opacity: 0.04, class: 'ring-tint' }, svg);
    el('circle', { r: outerR, fill: 'none', stroke: 'var(--glass-line)', 'stroke-width': .8, opacity: .7 }, svg);
    slots.forEach((i) => {
      const mid = -90 + i * step;
      const reach = outerR;
      this.wedges.push(el('path', { d: sector(innerR, reach, mid - step / 2, mid + step / 2), fill: `url(#${id}-hot)`, 'fill-opacity': 0, class: 'wedge' }, svg));
      const [x0, y0] = polar(outerR - 12, mid - step / 2);
      const [x1, y1] = polar(outerR, mid - step / 2);
      el('line', { x1: x0, y1: y0, x2: x1, y2: y1, stroke: 'var(--glass-line)', 'stroke-opacity': 0.22, 'stroke-width': 1.2 }, svg);
    });
    slots.forEach((i) => {
      const [ix, iy] = polar(iconR, -90 + i * step);
      this.plates.push(el('rect', { x: ix - 33, y: iy - 33, width: 66, height: 66, rx: 18, fill: '#fff', 'fill-opacity': 0, class: 'iconPlate' }, svg));
      this.icons.push(el('image', { x: ix - iconSize / 2, y: iy - iconSize / 2, width: iconSize, height: iconSize, class: 'icon' }, svg));
    });

    el('circle', { r: innerR, fill: 'var(--hub-tint)', stroke: 'var(--glass-line)', 'stroke-width': 1, class: 'core' }, svg);
    this.rig = el('g', { class: 'mascotRig' }, svg);
    const holder = el('foreignObject', { x: -46, y: -66, width: 92, height: 96 }, this.rig);
    const sprite = document.createElementNS('http://www.w3.org/1999/xhtml', 'canvas') as HTMLCanvasElement; sprite.className = 'mascotSprite';
    sprite.style.width = '92px'; sprite.style.height = '96px'; holder.append(sprite);
    this.mascot = new SpriteMascot(sprite);
    if (this.opened) this.mascot.start();
    this.centerTxt = el('text', { x: 0, y: 52, 'text-anchor': 'middle', class: 'centerTxt' }, svg);
    this.centerTxt.textContent = this.centerLabel;
    el('text', { x: 6, y: 75, 'text-anchor': 'middle', class: 'backTxt' }, svg).textContent = 'ESC';
    const mouse = el('g', { opacity: 0.55, transform: 'translate(-26,65)' }, svg);
    el('rect', { x: 0, y: 0, width: 11, height: 16, rx: 5.5, fill: 'none', stroke: '#6c7a7c', 'stroke-width': 1.3 }, mouse);
    el('path', { d: 'M5.5,0 V7 H11 V5.5 A5.5,5.5 0 0 0 5.5,0Z', fill: '#6c7a7c' }, mouse);
  }
}
