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
export interface WindowState { accessibility: boolean; screen: boolean; running: string[]; windows: WindowItem[]; }
export interface WindowChip { index: number; x: number; y: number; width: number; height: number; }
/** A compact fan beside the owning app, with the same hit rectangles in main and renderer. */
export function windowChips(cx: number, cy: number, slot: number, count: number, size: number): WindowChip[] {
  const n = Math.min(12, count), radius = size * .43;
  const middle = -Math.PI / 2 + slot * Math.PI / 4;
  const chips: WindowChip[] = [];
  const overlaps = (a: WindowChip, b: WindowChip): boolean => a.x < b.x + b.width + 5 && a.x + a.width + 5 > b.x && a.y < b.y + b.height + 5 && a.y + a.height + 5 > b.y;
  for (let index = 0; index < n; index++) {
    const sign = index % 2 ? 1 : -1;
    let distance = index === 0 ? 0 : .15 * Math.ceil(index / 2);
    let chip: WindowChip;
    do {
      const angle = middle + sign * distance;
      chip = { index, x: cx + Math.cos(angle) * radius - 70, y: cy + Math.sin(angle) * radius - 15, width: 140, height: 30 };
      distance += .05;
    } while (chips.some((other) => overlaps(chip, other)) && distance < Math.PI * 1.9);
    chips.push(chip);
  }
  return chips;
}
export function chipAt(x: number, y: number, chips: WindowChip[]): number | null {
  return chips.find((chip) => x >= chip.x && x <= chip.x + chip.width && y >= chip.y && y <= chip.y + chip.height)?.index ?? null;
}
