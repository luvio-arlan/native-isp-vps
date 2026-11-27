import React from "react";
import { Activity, RefreshCw } from "lucide-react";
import type { CarrierLineMetrics, CarrierQualitySnapshot, CarrierSample } from "../types/telemetry";

interface CarrierQualityCardProps {
  quality: CarrierQualitySnapshot | null;
  isProbing: boolean;
  onProbe: () => void;
}

const SLOTS = 24;

const FALLBACK_CARRIERS: CarrierLineMetrics[] = [
  { key: "ct", label: "电信", target: "202.96.209.133", latestRttMs: null, lossPct: 0, samples: [] },
  { key: "cu", label: "联通", target: "119.6.6.6", latestRttMs: null, lossPct: 0, samples: [] },
  { key: "cm", label: "移动", target: "211.136.112.50", latestRttMs: null, lossPct: 0, samples: [] },
];

function formatClock(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`;
}

function rttBarColor(rttMs: number | null | undefined, lossPct: number): string {
  if (rttMs == null || lossPct >= 100) return "bg-rose-500";
  if (rttMs <= 145) return "bg-emerald-500";
  if (rttMs <= 215) return "bg-lime-400";
  if (rttMs <= 280) return "bg-amber-400";
  return "bg-orange-500";
}

function rttTextColor(rttMs: number | null | undefined): string {
  if (rttMs == null) return "text-zinc-500";
  if (rttMs <= 145) return "text-emerald-400";
  if (rttMs <= 215) return "text-lime-400";
  if (rttMs <= 280) return "text-amber-400";
  return "text-rose-400";
}

function lossBarColor(lossPct: number): string {
  if (lossPct <= 0) return "bg-emerald-500";
  if (lossPct < 50) return "bg-rose-500";
  return "bg-rose-600";
}

function lossTextColor(lossPct: number): string {
  if (lossPct <= 0) return "text-emerald-400";
  if (lossPct < 5) return "text-amber-400";
  return "text-rose-400";
}

function buildPaddedSlots(samples: CarrierSample[]): Array<CarrierSample | null> {
  const trimmed = samples.slice(-SLOTS);
  const pad = Math.max(0, SLOTS - trimmed.length);
  return [...Array<CarrierSample | null>(pad).fill(null), ...trimmed];
}

export const CarrierQualityCard: React.FC<CarrierQualityCardProps> = React.memo(
  ({ quality, isProbing, onProbe }) => {
    const carriers =
      quality && quality.carriers.length > 0 ? quality.carriers : FALLBACK_CARRIERS;

    return (
      <div className="glass-card glass-card-hover rounded-2xl p-5">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2.5">
            <div className="flex items-center gap-2 text-[13px] font-semibold text-zinc-100">
              <Activity className="h-4 w-4 text-emerald-400" />
              三网回程质量
            </div>
            {quality?.updatedAt ? (
              <span className="font-mono-num text-[11px] text-zinc-500">
                更新于 {formatClock(quality.updatedAt)}
              </span>
            ) : null}
          </div>

          <button
            onClick={onProbe}
            disabled={isProbing}
            className="inline-flex items-center gap-1.5 rounded-xl border border-white/10 bg-white/6 px-3 py-1.5 text-[11.5px] font-medium text-zinc-200 transition hover:bg-white/12 disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 text-emerald-400 ${isProbing ? "animate-spin" : ""}`} />
            {isProbing ? "探测中..." : "立即测三网"}
          </button>
        </div>

        <div className="space-y-3.5">
          {carriers.map((line) => {
            const slots = buildPaddedSlots(line.samples);
            return (
              <div key={line.key} className="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-5">
                {/* Left: Latency Capsule Bar */}
                <div>
                  <div className="mb-1.5 flex items-baseline justify-between">
                    <div className="flex items-baseline gap-2">
                      <span className="text-[12.5px] font-medium text-zinc-200">
                        {line.label}
                      </span>
                      <span className="font-mono-num text-[10.5px] text-zinc-500">
                        {line.target}
                      </span>
                    </div>
                    <div className="font-mono-num text-[13px] font-semibold">
                      {line.latestRttMs != null ? (
                        <>
                          <span className={rttTextColor(line.latestRttMs)}>
                            {Math.round(line.latestRttMs)}
                          </span>
                          <span className="ml-0.5 text-[10.5px] font-normal text-zinc-500">
                            ms
                          </span>
                        </>
                      ) : (
                        <span className="text-zinc-500">-- ms</span>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-[3px]">
                    {slots.map((sample, idx) => (
                      <div
                        key={`rtt-${line.key}-${idx}`}
                        title={
                          sample
                            ? `${line.label} · ${formatClock(sample.ts)} · ${
                                sample.rttMs != null ? `${Math.round(sample.rttMs)} ms` : "超时"
                              } · 丢包 ${sample.lossPct.toFixed(1)}%`
                            : "等待采样"
                        }
                        className={`h-3 flex-1 rounded-[3px] transition-all duration-300 ${
                          sample
                            ? rttBarColor(sample.rttMs, sample.lossPct)
                            : "carrier-slot-empty"
                        }`}
                      />
                    ))}
                  </div>
                </div>

                {/* Right: Loss Rate Capsule Bar */}
                <div>
                  <div className="mb-1.5 flex items-baseline justify-between">
                    <span className="text-[11px] text-zinc-500">丢包率</span>
                    <div className="font-mono-num text-[13px] font-semibold">
                      <span className={lossTextColor(line.lossPct)}>
                        {line.lossPct.toFixed(1)}
                      </span>
                      <span className="ml-0.5 text-[10.5px] font-normal text-zinc-500">
                        %
                      </span>
                    </div>
                  </div>
                  <div className="flex items-center gap-[3px]">
                    {slots.map((sample, idx) => (
                      <div
                        key={`loss-${line.key}-${idx}`}
                        title={
                          sample
                            ? `${line.label} · ${formatClock(sample.ts)} · 丢包 ${sample.lossPct.toFixed(1)}%`
                            : "等待采样"
                        }
                        className={`h-3 flex-1 rounded-[3px] transition-all duration-300 ${
                          sample ? lossBarColor(sample.lossPct) : "carrier-slot-empty"
                        }`}
                      />
                    ))}
                  </div>
                </div>
              </div>
            );
          })}
        </div>

        {quality?.error ? (
          <div className="mt-3 rounded-xl border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-[11.5px] text-amber-300">
            最近一次三网探测提示：{quality.error}
          </div>
        ) : null}
      </div>
    );
  }
);
