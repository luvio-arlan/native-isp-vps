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

export function daysUntilReset(resetDay: number): number {
  const now = new Date();
  const safeDay = Math.max(1, Math.min(28, resetDay || 1));
  const currentMonthReset = new Date(now.getFullYear(), now.getMonth(), safeDay);
  let target = currentMonthReset;
  if (now.getDate() >= safeDay) {
    target = new Date(now.getFullYear(), now.getMonth() + 1, safeDay);
  }
  const diffMs = target.getTime() - now.getTime();
  return Math.max(1, Math.ceil(diffMs / 86400000));
}

export function daysElapsedInCycle(resetDay: number): number {
  const remaining = daysUntilReset(resetDay);
  return Math.max(1, 30 - remaining);
}

export function forecastCycleUsage(usedBytes: number, totalBytes: number, resetDay: number): {
  projectedBytes: number;
  willExceed: boolean;
  pct: number;
} {
  const elapsed = daysElapsedInCycle(resetDay);
  const dailyRate = usedBytes / elapsed;
  const projectedBytes = dailyRate * 30;
  const pct = totalBytes > 0 ? Math.min(100, (usedBytes / totalBytes) * 100) : 0;
  return {
    projectedBytes,
    willExceed: totalBytes > 0 && projectedBytes > totalBytes,
    pct,
  };
}
