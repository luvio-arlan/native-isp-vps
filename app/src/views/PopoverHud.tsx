import React, { useMemo, useState } from "react";
import {
  Activity,
  Check,
  Copy,
  Maximize2,
  ShieldCheck,
  ShieldAlert,
  Sparkles,
  Zap,
} from "lucide-react";
import { useTelemetry } from "../hooks/useTelemetry";
import { SparklineChart } from "../components/SparklineChart";
import { StatusBadge } from "../components/StatusBadge";
import { api } from "../lib/tauri";
import {
  daysUntilReset,
  formatBytes,
  formatSpeed,
  formatSpeedInline,
} from "../lib/format";

export const PopoverHud: React.FC = () => {
  const { activeNode, snapshot, isDemo, isProbing, runFullProbe } = useTelemetry();
  const [channel, setChannel] = useState<"local" | "vps">("local");
  const [copied, setCopied] = useState(false);

  const useLocal = channel === "local" && Boolean(snapshot?.localShield.mihomoConnected);
  const downBps = useLocal
    ? snapshot?.localShield.localDownBps ?? 0
    : snapshot?.xui.netDownBps ?? 0;
  const upBps = useLocal
    ? snapshot?.localShield.localUpBps ?? 0
    : snapshot?.xui.netUpBps ?? 0;

  const downFmt = formatSpeed(downBps);
  const upFmt = formatSpeed(upBps);

  const chartData = useMemo(
    () =>
      (snapshot?.history ?? []).map((pt) => ({
        ts: pt.ts,
        primary: useLocal ? pt.localDownBps : pt.vpsDownBps,
        secondary: useLocal ? pt.localUpBps : pt.vpsUpBps,
      })),
    [snapshot?.history, useLocal]
  );

  const primaryClient = snapshot?.clients[0];
  const primaryUsed = primaryClient ? primaryClient.upBytes + primaryClient.downBytes : 0;
  const primaryTotal = primaryClient?.totalBytes || activeNode.monthlyTotalGb * 1024 ** 3;
  const primaryPct = primaryTotal > 0 ? Math.min(100, (primaryUsed / primaryTotal) * 100) : 0;

  const memPct =
    snapshot && snapshot.xui.memTotal > 0
      ? Math.round((snapshot.xui.memUsed / snapshot.xui.memTotal) * 100)
      : 0;

  const shieldOk =
    snapshot?.localShield.ipv6Leak !== true &&
    snapshot?.localShield.tunEnabled !== false;

  const handleCopySub = async () => {
    const url = primaryClient?.subUrl || activeNode.subUrl;
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // Ignore clipboard permission errors in background popover
    }
  };

  return (
    <div className="h-screen w-screen bg-transparent px-5 pt-2 pb-7">
      <div className="popover-shell flex h-full w-full flex-col justify-between rounded-2xl p-3.5 text-zinc-100">
        {/* Top Node Header */}
        <div className="flex items-center justify-between gap-2">
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="text-[15px]">{activeNode.flag}</span>
              <span className="truncate text-[13px] font-semibold tracking-tight text-zinc-100">
                {activeNode.city} · {activeNode.ispTag}
              </span>
            </div>
            <div className="mt-0.5 flex items-center gap-2">
              <StatusBadge
                status={snapshot?.healthStatus ?? "demo"}
                label={
                  isDemo
                    ? `演示 · ${snapshot?.healthScore ?? 98}分`
                    : `${snapshot?.healthScore ?? "--"}分`
                }
              />
              <span className="font-mono-num text-[10.5px] text-zinc-500">
                {activeNode.host}:{activeNode.vlessPort}
              </span>
            </div>
          </div>

          <button
            onClick={() => api.openMainWindow()}
            title="展开完整驾驶舱"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl border border-white/10 bg-white/6 text-zinc-200 outline-none transition hover:border-emerald-500/40 hover:bg-emerald-500/15 hover:text-emerald-200 focus:outline-none focus-visible:ring-1 focus-visible:ring-emerald-400/50"
          >
            <Maximize2 className="h-3.5 w-3.5" />
          </button>
        </div>

        {/* Hero Live Speed + Sparkline */}
        <div className="glass-card mt-3 rounded-xl p-3">
          <div className="mb-1.5 flex items-center justify-between">
            <div className="flex items-baseline gap-3">
              <div>
                <div className="flex items-baseline gap-1 font-mono-num">
                  <span className="text-[11px] text-emerald-400">↓</span>
                  <span className="text-[22px] font-semibold leading-none tracking-tight text-zinc-50">
                    {downFmt.value}
                  </span>
                  <span className="text-[10.5px] text-zinc-400">{downFmt.unit}</span>
                </div>
              </div>
              <div className="flex items-baseline gap-1 font-mono-num">
                <span className="text-[10.5px] text-sky-400">↑</span>
                <span className="text-[14px] font-medium text-zinc-200">{upFmt.value}</span>
                <span className="text-[10px] text-zinc-500">{upFmt.unit}</span>
              </div>
            </div>

            <div className="flex rounded-lg bg-black/45 p-0.5 text-[10px]">
              <button
                onClick={() => setChannel("local")}
                className={`rounded-md px-2 py-0.5 transition ${
                  channel === "local" ? "bg-white/12 text-zinc-100" : "text-zinc-500 hover:text-zinc-300"
                }`}
              >
                本机
              </button>
              <button
                onClick={() => setChannel("vps")}
                className={`rounded-md px-2 py-0.5 transition ${
                  channel === "vps" ? "bg-white/12 text-zinc-100" : "text-zinc-500 hover:text-zinc-300"
                }`}
              >
                VPS
              </button>
            </div>
          </div>

          <SparklineChart
            data={chartData}
            height={92}
            compact
            formatValue={formatSpeedInline}
          />
        </div>

        {/* 3-Pill Telemetry Strip */}
        <div className="mt-2.5 grid grid-cols-3 gap-2">
          <div className="glass-card rounded-xl px-2.5 py-2">
            <div className="text-[10px] text-zinc-500">延迟 / 抖动</div>
            <div className="mt-0.5 font-mono-num text-[13px] font-semibold text-zinc-100">
              {snapshot?.rttMs != null ? `${Math.round(snapshot.rttMs)}ms` : "离线"}
            </div>
            <div className="truncate whitespace-nowrap font-mono-num text-[9.5px] tracking-tight text-zinc-400">
              ±{snapshot?.jitterMs ?? 0}ms · {snapshot?.packetLossPct ?? 0}%丢包
            </div>
          </div>

          <div className="glass-card rounded-xl px-2.5 py-2">
            <div className="text-[10px] text-zinc-500">VPS 负载</div>
            <div className="mt-0.5 font-mono-num text-[13px] font-semibold text-zinc-100">
              CPU {Math.round(snapshot?.xui.cpuPercent ?? 0)}%
            </div>
            <div className="truncate whitespace-nowrap font-mono-num text-[9.5px] tracking-tight text-zinc-400">
              RAM {memPct}% · {snapshot?.xui.tcpCount ?? 0} TCP
            </div>
          </div>

          <div className="glass-card rounded-xl px-2.5 py-2">
            <div className="text-[10px] text-zinc-500">防泄露盾</div>
            <div className="mt-0.5 flex items-center gap-1 text-[12px] font-semibold">
              {shieldOk ? (
                <>
                  <ShieldCheck className="h-3.5 w-3.5 text-emerald-400" />
                  <span className="text-emerald-300">安全</span>
                </>
              ) : (
                <>
                  <ShieldAlert className="h-3.5 w-3.5 text-rose-400" />
                  <span className="text-rose-300">告警</span>
                </>
              )}
            </div>
            <div className="truncate whitespace-nowrap font-mono-num text-[9.5px] tracking-tight text-zinc-400">
              {snapshot?.localShield.mihomoConnected
                ? `TUN ${snapshot.localShield.tunStack ?? "mixed"}`
                : "未连接"}
            </div>
          </div>
        </div>

        {/* Quota Progress */}
        <div className="glass-card mt-2.5 rounded-xl px-3 py-2.5">
          <div className="flex items-center justify-between text-[11px]">
            <span className="font-medium text-zinc-200">
              {primaryClient ? `配额 · ${primaryClient.email}` : "月度流量配额"}
            </span>
            <span className="font-mono-num text-zinc-400">
              {formatBytes(primaryUsed)} / {formatBytes(primaryTotal)}
            </span>
          </div>
          <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-white/8">
            <div
              className="h-full rounded-full bg-gradient-to-r from-emerald-500 to-sky-400 transition-all duration-500"
              style={{ width: `${Math.max(2, primaryPct)}%` }}
            />
          </div>
          <div className="mt-1.5 flex items-center justify-between text-[10px] text-zinc-500">
            <span className="font-mono-num">
              已用 {primaryPct.toFixed(1)}% · {daysUntilReset(activeNode.resetDay)} 天后重置
            </span>
            {snapshot && snapshot.clients.length > 1 && (
              <span>{snapshot.clients.length} 位成员</span>
            )}
          </div>
        </div>

        {/* Footer Quick Actions */}
        <div className="mt-3 grid grid-cols-3 gap-1.5">
          <button
            onClick={runFullProbe}
            disabled={isProbing}
            className="flex items-center justify-center gap-1 rounded-xl border border-white/8 bg-white/5 py-2 text-[11px] font-medium text-zinc-200 transition hover:bg-white/10 disabled:opacity-50"
          >
            {isProbing ? (
              <Activity className="h-3.5 w-3.5 animate-spin text-emerald-400" />
            ) : (
              <Zap className="h-3.5 w-3.5 text-emerald-400" />
            )}
            验真测速
          </button>

          <button
            onClick={handleCopySub}
            className="flex items-center justify-center gap-1 rounded-xl border border-white/8 bg-white/5 py-2 text-[11px] font-medium text-zinc-200 transition hover:bg-white/10"
          >
            {copied ? (
              <Check className="h-3.5 w-3.5 text-emerald-400" />
            ) : (
              <Copy className="h-3.5 w-3.5 text-sky-400" />
            )}
            {copied ? "已复制" : "复制订阅"}
          </button>

          <button
            onClick={() => api.openMainWindow()}
            className="flex items-center justify-center gap-1 rounded-xl bg-emerald-500/18 py-2 text-[11px] font-medium text-emerald-200 transition hover:bg-emerald-500/28"
          >
            <Sparkles className="h-3.5 w-3.5" />
            完整看板
          </button>
        </div>
      </div>
    </div>
  );
};
