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

/** Seconds to show now: the backend's value plus the time since it was fetched, while running. */
export function liveSeconds(base: number, running: boolean, fetchedAtMs: number, nowMs: number): number {
  if (!running) return base;
  return base + Math.max(0, Math.floor((nowMs - fetchedAtMs) / 1000));
}

/** Local calendar day of `d`, e.g. `2026-11-07`; used to notice midnight. */
export function localDateKey(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `YYYY-MM-DDTHH:MM` (the `datetime-local` format) in local time. */
export function toLocalInput(d: Date): string {
  return `${localDateKey(d)}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * Local `YYYY-MM-DDTHH:MM` plus `hours` of real time, in the same format (so a DST change in
 * between is accounted for). `null` when the input is not in that format.
 */
export function addHoursLocal(local: string, hours: number): string | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(local);
  if (!m) return null;
  const [y, mo, d, h, mi] = m.slice(1).map(Number);
  const start = new Date(y, mo - 1, d, h, mi);
  return toLocalInput(new Date(start.getTime() + hours * 3_600_000));
}

/** Minutes as the backend's `format_hm`: `20m`, `1h 5m`. */
export function formatMinutes(minutes: number): string {
  const m = Math.max(0, Math.floor(minutes));
  return m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${m % 60}m`;
}
