export function formatSpeed(bytesPerSec: number): { value: string; unit: string } {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec <= 0) {
    return { value: "0.0", unit: "KB/s" };
  }
  const gb = bytesPerSec / (1024 * 1024 * 1024);
  if (gb >= 1) return { value: gb.toFixed(2), unit: "GB/s" };
  const mb = bytesPerSec / (1024 * 1024);
  if (mb >= 1) return { value: mb.toFixed(mb >= 100 ? 0 : mb >= 10 ? 1 : 2), unit: "MB/s" };
  const kb = bytesPerSec / 1024;
  return { value: kb >= 10 ? kb.toFixed(0) : kb.toFixed(1), unit: "KB/s" };
}

export function formatSpeedInline(bytesPerSec: number): string {
  const { value, unit } = formatSpeed(bytesPerSec);
  return `${value} ${unit}`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let idx = 0;
  let val = bytes;
  while (val >= 1024 && idx < units.length - 1) {
    val /= 1024;
    idx++;
  }
  return `${val >= 100 ? val.toFixed(0) : val >= 10 ? val.toFixed(1) : val.toFixed(2)} ${units[idx]}`;
}

export function formatUptime(secs: number): string {
  if (!secs || secs <= 0) return "--";
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

const DAY_MS = 86_400_000;

/** Local-midnight reset date for `resetDay` (1..31) clamped to the month length (31 → Feb 28/29). */
function resetDateFor(year: number, month: number, resetDay: number): Date {
  const day = Math.max(1, Math.min(31, Math.round(resetDay) || 1));
  const lastDay = new Date(year, month + 1, 0).getDate();
  return new Date(year, month, Math.min(day, lastDay));
}

/** Current monthly billing cycle `[start, end)` in local time, matching 3x-ui `reset` semantics. */
export function billingCycle(resetDay: number, now: Date = new Date()): { start: Date; end: Date } {
  const y = now.getFullYear();
  const m = now.getMonth();
  const thisMonth = resetDateFor(y, m, resetDay);
  return now >= thisMonth
    ? { start: thisMonth, end: resetDateFor(y, m + 1, resetDay) }
    : { start: resetDateFor(y, m - 1, resetDay), end: thisMonth };
}

export function daysUntilReset(resetDay: number, now: Date = new Date()): number {
  const { end } = billingCycle(resetDay, now);
  return Math.max(1, Math.ceil((end.getTime() - now.getTime()) / DAY_MS));
}

export function forecastCycleUsage(
  usedBytes: number,
  totalBytes: number,
  resetDay: number,
  now: Date = new Date()
): {
  projectedBytes: number;
  willExceed: boolean;
  pct: number;
} {
  const { start, end } = billingCycle(resetDay, now);
  // At least one day elapsed so a fresh cycle never extrapolates a few minutes of traffic ×30.
  const elapsedMs = Math.max(DAY_MS, now.getTime() - start.getTime());
  const projectedBytes = (usedBytes / elapsedMs) * (end.getTime() - start.getTime());
  const pct = totalBytes > 0 ? Math.min(100, (usedBytes / totalBytes) * 100) : 0;
  return {
    projectedBytes,
    willExceed: totalBytes > 0 && projectedBytes > totalBytes,
    pct,
  };
}
