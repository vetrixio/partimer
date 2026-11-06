// Project colour palette; the index is the `color` stored by the backend (0–7).

export const COLORS = [
  "#E57373",
  "#F0A04B",
  "#E6C449",
  "#7CC35B",
  "#4DB6AC",
  "#64B5F6",
  "#9575CD",
  "#F06292",
] as const;

export function colorOf(index: number): string {
  return COLORS[index] ?? COLORS[0];
}
