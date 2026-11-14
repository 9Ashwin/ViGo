/** Original supplied sheet: 8 columns × 11 rows, 192 × 208 px cells. Pixels are preserved unchanged. */
export const MASCOT_SHEET = { width: 1536, height: 2288, cellWidth: 192 } as const;
export const MASCOT_ROWS = [
  {
    "frames": 7,
    "x": 25,
    "y": 64,
    "width": 141,
    "height": 132
  },
  {
    "frames": 8,
    "x": 40,
    "y": 292,
    "width": 112,
    "height": 112
  },
  {
    "frames": 8,
    "x": 33,
    "y": 501,
    "width": 125,
    "height": 111
  },
  {
    "frames": 4,
    "x": 41,
    "y": 708,
    "width": 109,
    "height": 112
  },
  {
    "frames": 5,
    "x": 23,
    "y": 896,
    "width": 145,
    "height": 132
  },
  {
    "frames": 8,
    "x": 22,
    "y": 1132,
    "width": 148,
    "height": 104
  },
  {
    "frames": 6,
    "x": 30,
    "y": 1312,
    "width": 132,
    "height": 132
  },
  {
    "frames": 6,
    "x": 8,
    "y": 1503,
    "width": 176,
    "height": 149
  },
  {
    "frames": 6,
    "x": 8,
    "y": 1714,
    "width": 176,
    "height": 146
  },
  {
    "frames": 8,
    "x": 51,
    "y": 1936,
    "width": 68,
    "height": 132
  },
  {
    "frames": 8,
    "x": 43,
    "y": 2145,
    "width": 68,
    "height": 131
  }
] as const;
export function mascotFrame(row: number, elapsed: number, fps = 6, reduced = false, restMs = 0) {
  const state = MASCOT_ROWS[Math.max(0, Math.min(MASCOT_ROWS.length - 1, Math.floor(row)))];
  const interval = 1000 / Math.max(1, fps);
  const cycle = state.frames * interval + Math.max(0, restMs);
  const frame = reduced ? 0 : Math.min(state.frames - 1, Math.floor((Math.max(0, elapsed) % cycle) / interval));
  return { x: state.x + frame * MASCOT_SHEET.cellWidth, y: state.y, width: state.width, height: state.height, frame };
}
