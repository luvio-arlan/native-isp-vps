import React, { useMemo } from "react";
import { Activity, CheckCircle2, Globe2, ShieldAlert, ShieldCheck, XCircle, Zap } from "lucide-react";
import type { AiProbeResult, LeakCheckResult, NodeConfig, TelemetrySnapshot } from "../types/telemetry";
import { SparklineChart } from "../components/SparklineChart";

interface NetworkTabProps {
  node: NodeConfig;
  snapshot: TelemetrySnapshot | null;
  aiProbes: AiProbeResult[];
  leakResult: LeakCheckResult | null;
  isProbing: boolean;
  onRunProbe: () => void;
}

export const NetworkTab: React.FC<NetworkTabProps> = ({
  node,
  snapshot,
  aiProbes,
  leakResult,
  isProbing,
  onRunProbe,
}) => {
  const rttSeries = useMemo(
    () =>
      (snapshot?.history ?? []).map((pt) => ({
        ts: pt.ts,
        primary: pt.rttMs ?? 0,
      })),
    [snapshot?.history]
  );

  const validRtts = (snapshot?.history ?? [])
    .map((p) => p.rttMs)
    .filter((v): v is number => v != null && v > 0);
  const minRtt = validRtts.length ? Math.min(...validRtts) : null;
  const maxRtt = validRtts.length ? Math.max(...validRtts) : null;
  const avgRtt = validRtts.length
    ? Math.round(validRtts.reduce((a, b) => a + b, 0) / validRtts.length)
    : null;

  const exitIpv4 = leakResult?.exitIpv4 ?? snapshot?.localShield.exitIpv4;
  const ipv6Leak = leakResult?.ipv6Leak ?? snapshot?.localShield.ipv6Leak ?? false;
  const matchesVps = leakResult?.matchesVpsIp ?? snapshot?.localShield.matchesVpsIp;

  return (
    <div className="space-y-4">
      {/* RTT Timeline */}
      <div className="glass-card rounded-2xl p-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2.5">
            <div className="flex items-center gap-2 text-[13px] font-semibold text-zinc-100">
              <Activity className="h-4 w-4 text-emerald-400" />
              TCP 链路延迟
            </div>
            <span className="font-mono-num text-[11px] text-zinc-500">
              {node.host}:{node.vlessPort}
            </span>
          </div>

          <div className="flex items-center gap-4 font-mono-num text-[12px]">
            <span className="text-zinc-400">
              最小 <strong className="text-emerald-300">{minRtt != null ? `${Math.round(minRtt)}ms` : "--"}</strong>
            </span>
            <span className="text-zinc-400">
              均值 <strong className="text-zinc-100">{avgRtt != null ? `${avgRtt}ms` : "--"}</strong>
            </span>
            <span className="text-zinc-400">
              峰值 <strong className="text-amber-300">{maxRtt != null ? `${Math.round(maxRtt)}ms` : "--"}</strong>
            </span>
            <span className="text-zinc-400">
              丢包 <strong className="text-zinc-100">{snapshot?.packetLossPct ?? 0}%</strong>
            </span>
          </div>
        </div>

        <div className="mt-4">
          <SparklineChart
            data={rttSeries}
            height={150}
            primaryColor="#38bdf8"
            primaryLabel="RTT"
            formatValue={(v) => `${Math.round(v)} ms`}
          />
        </div>
      </div>

      {/* AI Matrix + Leak Check */}
      <div className="grid grid-cols-12 gap-4">
        <div className="glass-card col-span-8 rounded-2xl p-5">
          <div className="mb-4 flex items-center justify-between">
            <h3 className="text-[13px] font-semibold text-zinc-100">AI 服务连通性</h3>
            <button
              onClick={onRunProbe}
              disabled={isProbing}
              className="inline-flex items-center gap-1.5 rounded-xl bg-emerald-500/15 px-3.5 py-1.5 text-[12px] font-medium text-emerald-200 transition hover:bg-emerald-500/25 disabled:opacity-50"
            >
              <Zap className={`h-3.5 w-3.5 ${isProbing ? "animate-pulse" : ""}`} />
              {isProbing ? "测速中..." : "开始测速"}
            </button>
          </div>

          {aiProbes.length === 0 ? (
            <div className="rounded-xl border border-dashed border-white/10 bg-black/25 px-4 py-8 text-center">
              <p className="text-[12px] text-zinc-400">
                点击右上角「开始测速」检测延迟
              </p>
            </div>
          ) : (
            <div className="space-y-2">
              {aiProbes.map((item) => (
                <div
                  key={item.host}
                  className="flex items-center justify-between rounded-xl border border-white/6 bg-black/30 px-3.5 py-2.5"
                >
                  <div className="flex items-center gap-2.5">
                    {item.ok ? (
                      <CheckCircle2 className="h-4 w-4 text-emerald-400" />
                    ) : (
                      <XCircle className="h-4 w-4 text-rose-400" />
                    )}
                    <div>
                      <div className="text-[12.5px] font-medium text-zinc-100">{item.name}</div>
                      <div className="font-mono-num text-[10.5px] text-zinc-500">{item.host}</div>
                    </div>
                  </div>

                  <div className="flex items-center gap-3 font-mono-num text-[12px]">
                    {item.statusCode && (
                      <span className="rounded-md bg-white/6 px-2 py-0.5 text-[10.5px] text-zinc-300">
                        HTTP {item.statusCode}
                      </span>
                    )}
                    <span
                      className={
                        !item.ok
                          ? "text-rose-300"
                          : (item.latencyMs ?? 999) < 350
                            ? "text-emerald-300"
                            : "text-amber-300"
                      }
                    >
                      {item.latencyMs != null ? `${Math.round(item.latencyMs)} ms` : item.error || "超时"}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Exit IP & IPv6 Leak Guard */}
        <div className="glass-card col-span-4 flex flex-col rounded-2xl p-5">
          <div className="flex items-center gap-2 text-[13px] font-semibold text-zinc-100">
            <Globe2 className="h-4 w-4 text-sky-400" />
            出口与防泄露
          </div>

          <div className="mt-4 space-y-2.5">
            <div className="rounded-xl border border-white/7 bg-black/30 p-3">
              <div className="text-[11px] text-zinc-500">出口 IPv4</div>
              <div className="mt-1 font-mono-num text-[14px] font-semibold text-zinc-100">
                {exitIpv4 || "未检测"}
              </div>
              {exitIpv4 && (
                <div
                  className={`mt-1 text-[11px] ${
                    matchesVps ? "text-emerald-300" : "text-amber-300"
                  }`}
                >
                  {matchesVps ? "✓ 与节点 IP 一致" : `节点 IP：${node.host}`}
                </div>
              )}
            </div>

            <div className="rounded-xl border border-white/7 bg-black/30 p-3">
              <div className="text-[11px] text-zinc-500">IPv6 状态</div>
              <div className="mt-1 flex items-center gap-1.5 text-[13px] font-semibold">
                {ipv6Leak ? (
                  <>
                    <ShieldAlert className="h-4 w-4 text-rose-400" />
                    <span className="text-rose-300">检测到 IPv6 泄露</span>
                  </>
                ) : (
                  <>
                    <ShieldCheck className="h-4 w-4 text-emerald-400" />
                    <span className="text-emerald-300">已阻断 IPv6</span>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
