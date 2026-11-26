import React from "react";

interface StatusBadgeProps {
  status: "optimal" | "degraded" | "critical" | "demo" | string;
  label?: string;
}

const CONFIG: Record<string, { dot: string; pill: string; text: string; defaultLabel: string }> = {
  optimal: {
    dot: "bg-emerald-400",
    pill: "border-emerald-500/25 bg-emerald-500/10",
    text: "text-emerald-300",
    defaultLabel: "极佳",
  },
  demo: {
    dot: "bg-sky-400",
    pill: "border-sky-500/25 bg-sky-500/10",
    text: "text-sky-300",
    defaultLabel: "演示预览",
  },
  degraded: {
    dot: "bg-amber-400",
    pill: "border-amber-500/25 bg-amber-500/10",
    text: "text-amber-300",
    defaultLabel: "波动",
  },
  critical: {
    dot: "bg-rose-400",
    pill: "border-rose-500/25 bg-rose-500/10",
    text: "text-rose-300",
    defaultLabel: "异常",
  },
};

export const StatusBadge: React.FC<StatusBadgeProps> = ({ status, label }) => {
  const style = CONFIG[status] ?? CONFIG.degraded;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[11px] font-medium ${style.pill} ${style.text}`}
    >
      <span className="relative flex h-2 w-2">
        <span className={`animate-pulse-ring absolute inline-flex h-full w-full rounded-full ${style.dot}`} />
        <span className={`relative inline-flex h-2 w-2 rounded-full ${style.dot}`} />
      </span>
      {label ?? style.defaultLabel}
    </span>
  );
};
