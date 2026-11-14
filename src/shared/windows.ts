export interface WindowItem {
  id: string;
  appPath: string;
  pid: number;
  index: number;
  title: string;
  rawTitle: string;
  minimized: boolean;
  windowId: number;
  bounds: number[];
}
export interface PreviewFrame { image: string; cached: boolean; state: 'loading' | 'live' | 'cached' | 'unavailable'; message?: string; }
export interface WindowState { accessibility: boolean; screen: boolean; running: string[]; windows: WindowItem[]; }
export interface WindowChip { index: number; x: number; y: number; width: number; height: number; angle: number; panel?: number; }
export interface ContentBounds { x: number; y: number; width: number; height: number; }
/** All entries are visible at once; larger lists use adjacent columns, with no pagination. */
export function windowChips(cx: number, cy: number, slot: number, count: number, size: number, slotCount = 8, viewport?: { width: number; height: number }): WindowChip[] {
  if (!count) return [];
  const availableWidth = viewport?.width ?? 1470, availableHeight = viewport?.height ?? 956;
  const maxRows = Math.max(1, Math.min(12, Math.floor((availableHeight - 24) / 38)));
  const columns = Math.ceil(count / maxRows), rows = Math.ceil(count / columns);
  const width = Math.max(48, Math.min(184, Math.floor((availableWidth - 24 - (columns - 1) * 8) / columns)));
  const height = 34, gap = 4, padding = 6, radius = size * 188 / 600;
  const groupWidth = columns * width + (columns - 1) * 8 + padding * 2;
  const groupHeight = rows * height + (rows - 1) * gap + padding * 2;
  const theta = -Math.PI / 2 + slot * Math.PI * 2 / slotCount, dx = Math.cos(theta), dy = Math.sin(theta);
  let x: number, y: number;
  if (Math.abs(dx) >= .65) {
    x = dx >= 0 ? cx + radius + 14 : cx - radius - 14 - groupWidth;
    y = cy + dy * size * .18 - groupHeight / 2;
  } else {
    x = cx + dx * size * .18 - groupWidth / 2;
    y = dy < 0 ? cy - radius - 14 - groupHeight : cy + radius + 14;
  }
  const clamp = (value: number, max: number) => Math.max(8, Math.min(max - 8, value));
  const hub = size * 86 / 600 + 8;
  const clearHub = (px: number, py: number, w: number, h: number) => Math.hypot(Math.max(px - cx, 0, cx - px - w), Math.max(py - cy, 0, cy - py - h)) >= hub;
  const at = (px: number, py: number, panel = 0) => Array.from({ length: count }, (_, index) => ({ index, x: px + padding + Math.floor(index / rows) * (width + 8), y: py + padding + (index % rows) * (height + gap), width, height, angle: 0, panel }));
  if (!viewport) return at(x, y);
  const candidates = [
    [x, y], [cx + radius + 14, cy - groupHeight / 2], [cx - radius - 14 - groupWidth, cy - groupHeight / 2],
    [cx - groupWidth / 2, cy - radius - 14 - groupHeight], [cx - groupWidth / 2, cy + radius + 14],
  ].map(([px, py]) => [clamp(px, availableWidth - groupWidth), clamp(py, availableHeight - groupHeight)]);
  const position = candidates.find(([px, py]) => px + groupWidth <= availableWidth - 8 && py + groupHeight <= availableHeight - 8 && clearHub(px, py, groupWidth, groupHeight));
  if (position) return at(position[0], position[1]);
  // When one panel cannot fit, split its columns around the cancel hub.
  const rightRoom = Math.max(0, availableWidth - 8 - cx - hub - 4), leftRoom = Math.max(0, cx - hub - 12);
  let rightColumns = 0, bestWidth = -1;
  for (let right = 0; right <= columns; right++) {
    const left = columns - right;
    const rw = right ? (rightRoom - padding * 2 - (right - 1) * 8) / right : Infinity;
    const lw = left ? (leftRoom - padding * 2 - (left - 1) * 8) / left : Infinity;
    const candidate = Math.min(184, rw, lw);
    if (candidate > bestWidth) { bestWidth = candidate; rightColumns = right; }
  }
  const splitWidth = Math.max(1, Math.floor(bestWidth)), leftColumns = columns - rightColumns;
  const rightWidth = rightColumns * splitWidth + Math.max(0, rightColumns - 1) * 8 + padding * 2;
  const leftWidth = leftColumns * splitWidth + Math.max(0, leftColumns - 1) * 8 + padding * 2;
  const rightX = Math.max(cx + hub + 4, Math.min(cx + radius + 14, availableWidth - 8 - rightWidth));
  const leftX = Math.max(8, Math.min(cx - radius - 14 - leftWidth, cx - hub - 4 - leftWidth));
  const top = clamp(cy - groupHeight / 2, availableHeight - groupHeight);
  return Array.from({ length: count }, (_, index) => {
    const column = Math.floor(index / rows), right = column < rightColumns, localColumn = right ? column : column - rightColumns;
    return { index, x: (right ? rightX : leftX) + padding + localColumn * (splitWidth + 8), y: top + padding + (index % rows) * (height + gap), width: splitWidth, height, angle: 0, panel: column + 1 };
  });
}
export function contentBounds(chips: readonly WindowChip[]): ContentBounds | null {
  if (!chips.length) return null;
  const x = Math.min(...chips.map((c) => c.x)) - 6, y = Math.min(...chips.map((c) => c.y)) - 6;
  return { x, y, width: Math.max(...chips.map((c) => c.x + c.width)) - x + 6, height: Math.max(...chips.map((c) => c.y + c.height)) - y + 6 };
}
export function contentPanels(chips: readonly WindowChip[]): ContentBounds[] {
  const groups = new Map<number, WindowChip[]>();
  chips.forEach((chip) => { const id = chip.panel ?? 0; const group = groups.get(id) ?? []; group.push(chip); groups.set(id, group); });
  return [...groups.values()].map((group) => contentBounds(group)!);
}
export function chipAt(x: number, y: number, chips: WindowChip[]): number | null {
  return chips.find((chip) => {
    const dx = x - chip.x - chip.width / 2, dy = y - chip.y - chip.height / 2;
    const a = -chip.angle * Math.PI / 180;
    const lx = dx * Math.cos(a) - dy * Math.sin(a), ly = dx * Math.sin(a) + dy * Math.cos(a);
    return Math.abs(lx) <= chip.width / 2 && Math.abs(ly) <= chip.height / 2;
  })?.index ?? null;
}
