import React from "react";

interface GaugeRingProps {
  value: number; // 0..100
  label: string;
  sublabel?: string;
  size?: number;
  invertColor?: boolean; // true for health score (higher is greener)
}

export const GaugeRing: React.FC<GaugeRingProps> = ({
  value,
  label,
  sublabel,
  size = 86,
  invertColor = false,
}) => {
  const clamped = Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0));
  const stroke = 6.5;
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (clamped / 100) * circumference;

  const color = invertColor
    ? clamped >= 85
      ? "#10b981"
      : clamped >= 60
        ? "#f59e0b"
        : "#f43f5e"
    : clamped >= 88
      ? "#f43f5e"
      : clamped >= 70
        ? "#f59e0b"
        : "#10b981";

  return (
    <div className="flex flex-col items-center gap-1.5">
      <div className="relative flex items-center justify-center" style={{ width: size, height: size }}>
        <svg width={size} height={size} className="-rotate-90">
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke="var(--gauge-track)"
            strokeWidth={stroke}
          />
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke={color}
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeDasharray={circumference}
            strokeDashoffset={offset}
            style={{
              transition: "stroke-dashoffset 450ms cubic-bezier(0.22, 1, 0.36, 1), stroke 250ms ease",
              filter: `drop-shadow(0 0 6px ${color}55)`,
            }}
          />
        </svg>
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <span className="font-mono-num text-[17px] font-semibold tracking-tight text-zinc-100">
            {Math.round(clamped)}
            <span className="text-[10px] font-normal text-zinc-400">{invertColor ? "" : "%"}</span>
          </span>
        </div>
      </div>
      <div className="text-center">
        <div className="text-[12px] font-medium text-zinc-200">{label}</div>
        {sublabel && <div className="font-mono-num text-[10px] text-zinc-500">{sublabel}</div>}
      </div>
    </div>
  );
};
