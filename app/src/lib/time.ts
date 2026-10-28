// Relative and clock times. The board carries absolute timestamps only (AD-5):
// the window re-renders these on its own tick, the backend never re-emits for
// clock motion.

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "now", "4 min", "2 h 10", "3 d". */
export function duration(ms: number): string {
  const safe = Math.max(0, ms);
  if (safe < MINUTE) return "now";
  if (safe < HOUR) return `${Math.floor(safe / MINUTE)} min`;
  if (safe < DAY) {
    const hours = Math.floor(safe / HOUR);
    const minutes = Math.floor((safe % HOUR) / MINUTE);
    return minutes === 0 ? `${hours} h` : `${hours} h ${String(minutes).padStart(2, "0")}`;
  }
  return `${Math.floor(safe / DAY)} d`;
}

/** A window the backend sends in milliseconds, in whole days: "30 days". */
export function days(ms: number): string {
  const count = Math.round(Math.max(0, ms) / DAY);
  return `${count} ${count === 1 ? "day" : "days"}`;
}

/** A window the backend sends in milliseconds, in whole hours: "24 h". */
export function hours(ms: number): string {
  return `${Math.round(Math.max(0, ms) / HOUR)} h`;
}

/** A run's length: "350 ms", "4.2 s", then as `duration` from a minute on. */
export function runDuration(ms: number): string {
  const safe = Math.max(0, ms);
  if (safe < 1000) return `${Math.round(safe)} ms`;
  if (safe < MINUTE) return `${(safe / 1000).toFixed(1)} s`;
  return duration(safe);
}

/** "3 min ago", or "just now". */
export function ago(atMs: number, nowMs: number): string {
  const text = duration(nowMs - atMs);
  return text === "now" ? "just now" : `${text} ago`;
}

/** Local clock time "14:20", with the weekday when not today. */
export function clock(atMs: number, nowMs: number): string {
  const at = new Date(atMs);
  const time = at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const sameDay = new Date(nowMs).toDateString() === at.toDateString();
  return sameDay ? time : `${at.toLocaleDateString([], { weekday: "short" })} ${time}`;
}

/** Share of the 7-day window already elapsed, 0-100, from its reset time. */
export function weekElapsedPercent(resetsAtS: number, nowMs: number): number {
  const windowMs = 7 * DAY;
  const startMs = resetsAtS * 1000 - windowMs;
  return Math.min(100, Math.max(0, Math.round(((nowMs - startMs) / windowMs) * 100)));
}
