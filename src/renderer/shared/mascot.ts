import sheet from '../assets/mascot-sheet.png';
import { mascotFrame } from '../../shared/mascotFrames';
let sheetReady: Promise<HTMLImageElement> | null = null;
function loadSheet(): Promise<HTMLImageElement> {
  if (!sheetReady) { const image = new Image(); image.src = sheet; sheetReady = image.decode().then(() => image); }
  return sheetReady;
}
/** Draw only the current source rectangle into a small canvas; the original sheet stays unchanged. */
export class SpriteMascot {
  private frame: ReturnType<typeof setTimeout> | null = null;
  private running = false;
  private opened = 0;
  private enteredHub = 0;
  private hub = true;
  private lastMotion = 0;
  private dx = 0;
  private dy = 0;
  private row = -1;
  private changed = 0;
  private forced: number | null = null;
  private painted = '';
  private reduced = matchMedia('(prefers-reduced-motion: reduce)');
  private image: HTMLImageElement | null = null;
  private readonly context: CanvasRenderingContext2D | null;
  constructor(private readonly canvas: HTMLCanvasElement, width = 92, height = 96) {
    const scale = Math.min(2, devicePixelRatio || 1);
    canvas.width = Math.ceil(width * scale); canvas.height = Math.ceil(height * scale);
    canvas.setAttribute('aria-hidden', 'true'); this.context = canvas.getContext('2d', { willReadFrequently: true });
  }
  emote(row: number): void { this.forced = row; }
  start(): void {
    if (this.running) return;
    if (!this.image) void loadSheet().then((image) => { this.image = image; this.painted = ''; if (this.running) this.paint(this.row >= 0 ? this.row : 3, 0); }).catch(() => undefined);
    this.running = true; this.opened = performance.now(); this.enteredHub = 0;
    this.row = -1; this.render(this.opened); this.schedule();
  }
  stop(): void { this.running = false; if (this.frame) clearTimeout(this.frame); this.frame = null; }
  point(dx: number, dy: number, hub: boolean): void {
    if (hub && !this.hub) this.enteredHub = performance.now();
    if (Math.hypot(dx - this.dx, dy - this.dy) > .5) this.lastMotion = performance.now();
    this.dx = dx; this.dy = dy; this.hub = hub;
    if (this.running) { this.render(performance.now()); if (!this.frame) this.schedule(); }
  }
  private schedule(): void {
    if (!this.running || this.reduced.matches || this.frame) return;
    this.frame = setTimeout(() => { this.frame = null; this.render(performance.now()); this.schedule(); }, [0, 3, 5, 7].includes(this.row) ? 250 : 167);
  }
  private render(now: number): void {
    if (!this.running) return;
    const age = now - this.opened;
    let row = this.forced ?? 3;
    if (!this.reduced.matches && this.forced === null) {
      if (this.hub) {
        if (this.enteredHub && now - this.enteredHub < 1100) row = 4;
        else if (age < 800) row = 6;
        else row = [3, 0, 5, 7][Math.floor(age / 8000) % 4];
      } else if (Math.abs(this.dy) > Math.abs(this.dx) * 1.3) row = this.dy < 0 ? 9 : 10;
      else row = this.dx < 0 ? 2 : 1;
    }
    if (row !== this.row) { this.row = row; this.changed = now; this.canvas.dataset.state = String(row); }
    const time = !this.hub && now - this.lastMotion > 350 ? Math.max(0, this.lastMotion + 350 - this.changed) : now - this.changed;
    this.paint(row, time);
  }
  private paint(row: number, elapsed: number): void {
    const idle = [0, 3, 5, 7].includes(row);
    const frame = mascotFrame(row, elapsed, row === 3 ? 3 : idle ? 4 : 6, this.reduced.matches, idle ? 1000 : 0);
    const box = `${frame.x} ${frame.y} ${frame.width} ${frame.height}`;
    if (box === this.painted) return;
    if (!this.image || !this.context) return;
    this.painted = box; this.canvas.dataset.frame = String(frame.frame);
    const width = this.canvas.width, height = this.canvas.height;
    const scale = Math.min(width / frame.width, height / frame.height);
    const w = frame.width * scale, h = frame.height * scale;
    this.context.clearRect(0, 0, width, height);
    this.context.drawImage(this.image, frame.x, frame.y, frame.width, frame.height, (width - w) / 2, height - h, w, h);
  }
}
