// Clock-style durations for the per-second display. Negative input counts as 0.

function parts(seconds: number): [number, number, number] {
  const s = Math.max(0, Math.floor(seconds));
  return [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60];
}

const pad = (n: number) => String(n).padStart(2, "0");

/** `1:23:45` */
export function formatHms(seconds: number): string {
  const [h, m, s] = parts(seconds);
  return `${h}:${pad(m)}:${pad(s)}`;
}

/** `1:23` */
export function formatHm(seconds: number): string {
  const [h, m] = parts(seconds);
  return `${h}:${pad(m)}`;
}
