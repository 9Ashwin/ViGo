/** Wheel geometry in CSS pixels (matches prototype/wheel-demo.html). */
export const WHEEL = { size: 600, innerR: 86, outerR: 188, extR: 288, iconR: 137, iconSize: 48 } as const;

/**
 * Slice under an offset from the wheel centre. Screen coordinates (y down).
 * Returns -1 inside the dead zone, else 0..7 where 0 = N and indices go clockwise.
 * Slices are 45° wide and centred on the compass directions.
 */
export function pickSlice(dx: number, dy: number, deadZone: number = WHEEL.innerR): number {
  if (Math.hypot(dx, dy) < deadZone) return -1;
  const deg = ((Math.atan2(dy, dx) * 180) / Math.PI + 90 + 22.5 + 360) % 360;
  return Math.floor(deg / 45) % 8;
}

/**
 * The wheel's geometry: one ring of equal sectors around a dead hub. Pure maths, shared by the overlay
 * (which draws it) and the main process (which decides what the cursor is over), so the two can never
 * disagree about where a slice is.
 *
 * Angles are screen angles: 0 points right, they grow clockwise (the same direction as atan2(dy, dx)
 * with y pointing down). Slice 0 starts at the top.
 */

export const INNER_RADIUS = WHEEL.innerR;
export const OUTER_RADIUS = WHEEL.extR;
/** Room around the ring for the icons' glow and the hover label. */
export const PADDING = 26;
export const WINDOW_SIZE = 760;

export interface WheelBox {
  /** Centre of the ring inside the overlay window, in CSS pixels. */
  cx: number;
  cy: number;
  count: number;
  inner: number;
  outer: number;
}

/** Keep the press point at the hub even when the overlay window is clamped at a display edge. */
export function wheelBoxAt(x: number, y: number, originX: number, originY: number, size: number): WheelBox {
  return { cx: x - originX, cy: y - originY, count: 8,
    inner: WHEEL.innerR * size / WHEEL.size, outer: WHEEL.extR * size / WHEEL.size };
}

const TOP = -Math.PI / 2;

/** The slice at a window-local position, accounting for the ring's centre in that window. */
export function slotAt(x: number, y: number, box: WheelBox): number | null {
  const dx = x - box.cx;
  const dy = y - box.cy;
  const r = Math.hypot(dx, dy);
  if (r < box.inner || r > box.outer || box.count === 0) return null;
  const step = (Math.PI * 2) / box.count;
  // atan2 grows from -π at "left" through +π; moving it to start at the top keeps slice 0 up there.
  let a = Math.atan2(dy, dx) - TOP + step / 2;
  a = ((a % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
  return Math.min(box.count - 1, Math.floor(a / step));
}

/** Start and end angle (radians) of slice `i`, for the canvas. */
export function slotAngles(i: number, count: number): { start: number; end: number } {
  const step = (Math.PI * 2) / count;
  return { start: TOP + (i - 0.5) * step, end: TOP + (i + 0.5) * step };
}

/** Where slice `i`'s icon sits: on the ring's midline. */
export function slotIconCentre(i: number, count: number, box: WheelBox): { x: number; y: number } {
  const { start, end } = slotAngles(i, count);
  const mid = (start + end) / 2;
  const radius = (box.inner + box.outer) / 2;
  return { x: box.cx + Math.cos(mid) * radius, y: box.cy + Math.sin(mid) * radius };
}
