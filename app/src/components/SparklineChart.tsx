import React, { useId, useMemo, useState } from "react";

interface SeriesPoint {
  ts: number;
  primary: number;
  secondary?: number;
}

interface SparklineChartProps {
  data: SeriesPoint[];
  height?: number;
  primaryColor?: string;
  secondaryColor?: string;
  primaryLabel?: string;
  secondaryLabel?: string;
  formatValue: (val: number) => string;
  formatTime?: (ts: number) => string;
  showGrid?: boolean;
  compact?: boolean;
}

function buildSmoothPath(points: Array<{ x: number; y: number }>): string {
  if (points.length === 0) return "";
  if (points.length === 1) return `M ${points[0].x} ${points[0].y}`;
  let d = `M ${points[0].x.toFixed(1)} ${points[0].y.toFixed(1)}`;
  for (let i = 0; i < points.length - 1; i++) {
    const p0 = points[Math.max(0, i - 1)];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[Math.min(points.length - 1, i + 2)];
    const cp1x = p1.x + (p2.x - p0.x) / 6;
    const cp1y = p1.y + (p2.y - p0.y) / 6;
    const cp2x = p2.x - (p3.x - p1.x) / 6;
    const cp2y = p2.y - (p3.y - p1.y) / 6;
    d += ` C ${cp1x.toFixed(1)} ${cp1y.toFixed(1)}, ${cp2x.toFixed(1)} ${cp2y.toFixed(1)}, ${p2.x.toFixed(1)} ${p2.y.toFixed(1)}`;
  }
  return d;
}

export const SparklineChart: React.FC<SparklineChartProps> = React.memo(({
  data,
  height = 148,
  primaryColor = "#10b981",
  secondaryColor = "#38bdf8",
  primaryLabel = "下行",
  secondaryLabel = "上行",
  formatValue,
  formatTime,
  showGrid = true,
  compact = false,
}) => {
  const uid = useId().replace(/:/g, "");
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const width = compact ? 320 : 620;
  const padX = 4;
  const padTop = 12;
  const padBottom = 6;
  const innerW = width - padX * 2;
  const innerH = height - padTop - padBottom;
  const bottomY = padTop + innerH;

  // Memoize both coordinates AND SVG Bézier path strings so hover mousemove at 60fps
  // never recomputes curve geometry.
  const computed = useMemo(() => {
    if (data.length === 0) {
      return {
        primaryPts: [] as Array<{ x: number; y: number }>,
        primaryLine: "",
        secondaryLine: "",
        primaryArea: "",
        secondaryArea: "",
      };
    }
    let maxVal = 0;
    for (const d of data) {
      if (d.primary > maxVal) maxVal = d.primary;
      if (d.secondary != null && d.secondary > maxVal) maxVal = d.secondary;
    }
    maxVal = Math.max(maxVal * 1.18, 1);

    const stepX = data.length > 1 ? innerW / (data.length - 1) : innerW;
    const primaryPts = data.map((d, i) => ({
      x: padX + i * stepX,
      y: padTop + innerH - (d.primary / maxVal) * innerH,
    }));
    const secondaryPts = data.flatMap((d, i) =>
      d.secondary != null
        ? [
            {
              x: padX + i * stepX,
              y: padTop + innerH - (d.secondary / maxVal) * innerH,
            },
          ]
        : []
    );

    const primaryLine = buildSmoothPath(primaryPts);
    const secondaryLine = buildSmoothPath(secondaryPts);
    const primaryArea =
      primaryPts.length > 1
        ? `${primaryLine} L ${primaryPts[primaryPts.length - 1].x.toFixed(1)} ${bottomY} L ${primaryPts[0].x.toFixed(1)} ${bottomY} Z`
        : "";
    const secondaryArea =
      secondaryPts.length > 1
        ? `${secondaryLine} L ${secondaryPts[secondaryPts.length - 1].x.toFixed(1)} ${bottomY} L ${secondaryPts[0].x.toFixed(1)} ${bottomY} Z`
        : "";

    return { primaryPts, primaryLine, secondaryLine, primaryArea, secondaryArea };
  }, [data, innerW, innerH, bottomY]);

  const { primaryLine, secondaryLine, primaryArea, secondaryArea } = computed;
  const lastPrimary = computed.primaryPts[computed.primaryPts.length - 1];
  const activePoint = hoverIdx != null && data[hoverIdx] ? data[hoverIdx] : null;
  const activeCoord = hoverIdx != null ? computed.primaryPts[hoverIdx] : null;

  return (
    <div className="relative w-full select-none">
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full overflow-visible"
        style={{ height }}
        onMouseLeave={() => setHoverIdx(null)}
        onMouseMove={(e) => {
          if (data.length < 2) return;
          const rect = e.currentTarget.getBoundingClientRect();
          const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
          setHoverIdx(Math.round(ratio * (data.length - 1)));
        }}
      >
        <defs>
          <linearGradient id={`pgrad-${uid}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={primaryColor} stopOpacity="0.34" />
            <stop offset="85%" stopColor={primaryColor} stopOpacity="0.03" />
            <stop offset="100%" stopColor={primaryColor} stopOpacity="0" />
          </linearGradient>
          <linearGradient id={`sgrad-${uid}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={secondaryColor} stopOpacity="0.22" />
            <stop offset="100%" stopColor={secondaryColor} stopOpacity="0" />
          </linearGradient>
        </defs>

        {showGrid &&
          [0.25, 0.5, 0.75].map((ratio) => {
            const y = padTop + innerH * ratio;
            return (
              <line
                key={ratio}
                x1={padX}
                x2={width - padX}
                y1={y}
                y2={y}
                stroke="var(--chart-grid)"
                strokeDasharray="3 4"
                strokeWidth="1"
              />
            );
          })}

        {secondaryArea && <path d={secondaryArea} fill={`url(#sgrad-${uid})`} />}
        {primaryArea && <path d={primaryArea} fill={`url(#pgrad-${uid})`} />}

        {secondaryLine && (
          <path
            d={secondaryLine}
            fill="none"
            stroke={secondaryColor}
            strokeWidth={compact ? 1.4 : 1.75}
            strokeLinecap="round"
            opacity={0.85}
          />
        )}
        {primaryLine && (
          <path
            d={primaryLine}
            fill="none"
            stroke={primaryColor}
            strokeWidth={compact ? 1.8 : 2.2}
            strokeLinecap="round"
          />
        )}

        {activeCoord && (
          <line
            x1={activeCoord.x}
            x2={activeCoord.x}
            y1={padTop}
            y2={bottomY}
            stroke="var(--chart-cursor)"
            strokeWidth="1"
            strokeDasharray="2 3"
          />
        )}

        {lastPrimary && !activeCoord && (
          <g>
            <circle cx={lastPrimary.x} cy={lastPrimary.y} r="6" fill={primaryColor} opacity="0.2" />
            <circle cx={lastPrimary.x} cy={lastPrimary.y} r="2.8" fill={primaryColor} />
          </g>
        )}

        {activeCoord && (
          <circle
            cx={activeCoord.x}
            cy={activeCoord.y}
            r="3.6"
            fill="var(--chart-dot-fill)"
            stroke={primaryColor}
            strokeWidth="2"
          />
        )}
      </svg>

      {activePoint && (
        <div
          className="pointer-events-none absolute right-2 top-1 rounded-lg px-2.5 py-1 text-[11px] shadow-xl backdrop-blur-md"
          style={{
            background: "var(--tooltip-bg)",
            border: "1px solid var(--tooltip-border)",
          }}
        >
          <div className="flex items-center gap-3 font-mono-num">
            {formatTime && (
              <span className="text-zinc-400">{formatTime(activePoint.ts)}</span>
            )}
            <span className="flex items-center gap-1.5 text-zinc-200">
              <span className="h-1.5 w-1.5 rounded-full" style={{ background: primaryColor }} />
              {primaryLabel} {formatValue(activePoint.primary)}
            </span>
            {activePoint.secondary != null && (
              <span className="flex items-center gap-1.5 text-zinc-300">
                <span className="h-1.5 w-1.5 rounded-full" style={{ background: secondaryColor }} />
                {secondaryLabel} {formatValue(activePoint.secondary)}
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
});
