import React, { useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Check,
  Copy,
  Cpu,
  Globe,
  QrCode,
  Radio,
  ShieldCheck,
  Wifi,
} from "lucide-react";
import type { CarrierQualitySnapshot, NodeConfig, TelemetrySnapshot } from "../types/telemetry";
import { SparklineChart } from "../components/SparklineChart";
import { GaugeRing } from "../components/GaugeRing";
import { CarrierQualityCard } from "../components/CarrierQualityCard";
import {
  daysUntilReset,
  forecastCycleUsage,
  formatBytes,
  formatSpeed,
  formatSpeedInline,
  formatUptime,
} from "../lib/format";

interface OverviewTabProps {
  node: NodeConfig;
  snapshot: TelemetrySnapshot | null;
  carrierQuality: CarrierQualitySnapshot | null;
  isProbingCarriers: boolean;
  onProbeCarriers: () => void;
  onShowQr: (title: string, url: string) => void;
  onToast: (msg: string) => void;
}

export const OverviewTab: React.FC<OverviewTabProps> = ({
  node,
  snapshot,
  carrierQuality,
  isProbingCarriers,
  onProbeCarriers,
  onShowQr,
  onToast,
}) => {
  const [source, setSource] = useState<"local" | "vps">("local");
  const [copied, setCopied] = useState(false);

  const useLocal = source === "local" && Boolean(snapshot?.localShield.mihomoConnected);
  const downBps = useLocal
    ? snapshot?.localShield.localDownBps ?? 0
    : snapshot?.xui.netDownBps ?? 0;
  const upBps = useLocal
    ? snapshot?.localShield.localUpBps ?? 0
    : snapshot?.xui.netUpBps ?? 0;

  const downFmt = formatSpeed(downBps);
  const upFmt = formatSpeed(upBps);

  const speedSeries = useMemo(
    () =>
      (snapshot?.history ?? []).map((pt) => ({
        ts: pt.ts,
        primary: useLocal ? pt.localDownBps : pt.vpsDownBps,
        secondary: useLocal ? pt.localUpBps : pt.vpsUpBps,
      })),
    [snapshot?.history, useLocal]
  );

  const xui = snapshot?.xui;
  const memPct = xui && xui.memTotal > 0 ? (xui.memUsed / xui.memTotal) * 100 : 0;
  const swapPct = xui && xui.swapTotal > 0 ? (xui.swapUsed / xui.swapTotal) * 100 : 0;
  const diskPct = xui && xui.diskTotal > 0 ? (xui.diskUsed / xui.diskTotal) * 100 : 0;

  const totalUsedBytes = (snapshot?.clients ?? []).reduce(
    (acc, c) => acc + c.upBytes + c.downBytes,
    0
  );
  const totalQuotaBytes = node.monthlyTotalGb * 1024 ** 3;
  const forecast = forecastCycleUsage(totalUsedBytes, totalQuotaBytes, node.resetDay);

  const subUrl = snapshot?.clients[0]?.subUrl || node.subUrl;

  const handleCopySub = async () => {
    if (!subUrl) {
      onToast("当前节点未配置订阅链接");
      return;
    }
    await navigator.clipboard.writeText(subUrl);
    setCopied(true);
    onToast("万能订阅链接已复制到剪贴板");
    setTimeout(() => setCopied(false), 1800);
  };

  return (
    <div className="space-y-4">
      {/* Row 1: Hero Throughput + Health Radar */}
      <div className="grid grid-cols-12 gap-4">
        <div className="glass-card glass-card-hover col-span-8 rounded-2xl p-5">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <div className="flex items-center gap-2 text-[12px] font-medium text-zinc-400">
                <Radio className="h-3.5 w-3.5 text-emerald-400" />
                实时吞吐
              </div>
              <div className="mt-2.5 flex items-baseline gap-6">
                <div className="flex items-baseline gap-1.5 font-mono-num">
                  <ArrowDown className="h-4 w-4 text-emerald-400" />
                  <span className="text-[32px] font-semibold leading-none tracking-tight text-zinc-50">
                    {downFmt.value}
                  </span>
                  <span className="text-[12px] text-zinc-400">{downFmt.unit}</span>
                </div>
                <div className="flex items-baseline gap-1.5 font-mono-num">
                  <ArrowUp className="h-3.5 w-3.5 text-sky-400" />
                  <span className="text-[20px] font-semibold leading-none text-zinc-200">
                    {upFmt.value}
                  </span>
                  <span className="text-[11px] text-zinc-500">{upFmt.unit}</span>
                </div>
              </div>
            </div>

            <div className="flex items-center gap-1 rounded-xl border border-white/8 bg-black/35 p-1 text-[11.5px]">
              <button
                onClick={() => setSource("local")}
                className={`rounded-lg px-2.5 py-1 font-medium transition ${
                  source === "local"
                    ? "bg-white/12 text-zinc-100"
                    : "text-zinc-500 hover:text-zinc-300"
                }`}
              >
                本机
              </button>
              <button
                onClick={() => setSource("vps")}
                className={`rounded-lg px-2.5 py-1 font-medium transition ${
                  source === "vps"
                    ? "bg-white/12 text-zinc-100"
                    : "text-zinc-500 hover:text-zinc-300"
                }`}
              >
                VPS
              </button>
            </div>
          </div>

          <div className="mt-4">
            <SparklineChart
              data={speedSeries}
              height={162}
              formatValue={formatSpeedInline}
            />
          </div>

          <div className="mt-3 flex items-center justify-between border-t border-white/6 pt-3 text-[11px] text-zinc-400">
            <div className="flex items-center gap-4 font-mono-num">
              <span>
                累计发送{" "}
                {formatBytes(
                  useLocal
                    ? snapshot?.localShield.localUpTotal ?? 0
                    : xui?.netSentTotal ?? 0
                )}
              </span>
              <span>
                累计接收{" "}
                {formatBytes(
                  useLocal
                    ? snapshot?.localShield.localDownTotal ?? 0
                    : xui?.netRecvTotal ?? 0
                )}
              </span>
            </div>
            <div className="font-mono-num text-zinc-500">
              {useLocal
                ? `代理 ${snapshot?.localShield.proxyConnections ?? 0} · 直连 ${snapshot?.localShield.directConnections ?? 0}`
                : `TCP ${xui?.tcpCount ?? 0} · UDP ${xui?.udpCount ?? 0}`}
            </div>
          </div>
        </div>

        {/* Health Score & RTT Card */}
        <div className="glass-card glass-card-hover col-span-4 flex flex-col justify-between rounded-2xl p-5">
          <div className="flex items-center justify-between">
            <span className="text-[12px] font-medium text-zinc-400">
              节点健康
            </span>
            <span className="font-mono-num text-[11px] text-zinc-500">
              Xray {xui?.xrayVersion || "Reality"}
            </span>
          </div>

          <div className="my-2 flex justify-center">
            <GaugeRing
              value={snapshot?.healthScore ?? 0}
              label={
                (snapshot?.healthScore ?? 0) >= 85
                  ? "健康稳定"
                  : (snapshot?.healthScore ?? 0) >= 60
                    ? "轻微波动"
                    : "需要排查"
              }
              sublabel={`Uptime ${formatUptime(xui?.uptimeSecs ?? 0)}`}
              size={112}
              invertColor
            />
          </div>

          <div className="grid grid-cols-3 gap-2 border-t border-white/6 pt-3 text-center">
            <div>
              <div className="text-[10.5px] text-zinc-500">延迟</div>
              <div className="mt-0.5 font-mono-num text-[13px] font-semibold text-zinc-100">
                {snapshot?.rttMs != null ? `${Math.round(snapshot.rttMs)}ms` : "--"}
              </div>
            </div>
            <div>
              <div className="text-[10.5px] text-zinc-500">抖动</div>
              <div className="mt-0.5 font-mono-num text-[13px] font-semibold text-zinc-100">
                ±{snapshot?.jitterMs ?? 0}ms
              </div>
            </div>
            <div>
              <div className="text-[10.5px] text-zinc-500">丢包</div>
              <div
                className={`mt-0.5 font-mono-num text-[13px] font-semibold ${
                  (snapshot?.packetLossPct ?? 0) > 0 ? "text-amber-300" : "text-emerald-300"
                }`}
              >
                {snapshot?.packetLossPct ?? 0}%
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Row 2: VPS Hardware Rings + Quota Forecast */}
      <div className="grid grid-cols-12 gap-4">
        <div className="glass-card glass-card-hover col-span-7 rounded-2xl p-5">
          <div className="mb-4 flex items-center justify-between">
            <div className="flex items-center gap-2 text-[12px] font-medium text-zinc-200">
              <Cpu className="h-4 w-4 text-sky-400" />
              硬件状态
            </div>
            <span className="font-mono-num text-[11px] text-zinc-500">
              Load {xui ? xui.loads.map((v) => v.toFixed(2)).join(" / ") : "--"}
            </span>
          </div>

          <div className="grid grid-cols-4 gap-3">
            <GaugeRing value={xui?.cpuPercent ?? 0} label="CPU" />
            <GaugeRing
              value={memPct}
              label="内存"
              sublabel={xui ? `${formatBytes(xui.memUsed)} / ${formatBytes(xui.memTotal)}` : "--"}
            />
            <GaugeRing
              value={swapPct}
              label="Swap"
              sublabel={xui && xui.swapTotal > 0 ? formatBytes(xui.swapUsed) : "未启用"}
            />
            <GaugeRing
              value={diskPct}
              label="磁盘"
              sublabel={xui ? formatBytes(xui.diskUsed) : "--"}
            />
          </div>
        </div>

        <div className="glass-card glass-card-hover col-span-5 flex flex-col justify-between rounded-2xl p-5">
          <div>
            <div className="flex items-center justify-between">
              <span className="text-[12px] font-medium text-zinc-200">月度流量</span>
              <span className="font-mono-num text-[11px] text-zinc-400">
                剩余 {daysUntilReset(node.resetDay)} 天
              </span>
            </div>

            <div className="mt-3 flex items-baseline justify-between font-mono-num">
              <span className="text-[22px] font-semibold text-zinc-100">
                {formatBytes(totalUsedBytes)}
              </span>
              <span className="text-[12px] text-zinc-400">/ {node.monthlyTotalGb} GB</span>
            </div>

            <div className="mt-2 h-2 w-full overflow-hidden rounded-full bg-white/8">
              <div
                className={`h-full rounded-full transition-all duration-500 ${
                  forecast.pct > 85
                    ? "bg-gradient-to-r from-amber-500 to-rose-500"
                    : "bg-gradient-to-r from-emerald-500 to-sky-400"
                }`}
                style={{ width: `${Math.max(2, forecast.pct)}%` }}
              />
            </div>

            <div className="mt-2.5 flex items-center justify-between text-[11.5px]">
              <span className="font-mono-num text-zinc-400">
                预计月耗 {formatBytes(forecast.projectedBytes)}
              </span>
              <span className={forecast.willExceed ? "font-medium text-amber-300" : "font-medium text-emerald-300"}>
                {forecast.willExceed ? "预计偏紧" : "配额充裕"}
              </span>
            </div>
          </div>

          <div className="mt-3 flex gap-2 border-t border-white/6 pt-3">
            <button
              onClick={handleCopySub}
              className="flex flex-1 items-center justify-center gap-1.5 rounded-xl border border-white/10 bg-white/6 py-2 text-[12px] font-medium text-zinc-100 transition hover:bg-white/12"
            >
              {copied ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5 text-sky-400" />}
              {copied ? "已复制" : "复制订阅"}
            </button>
            {subUrl && (
              <button
                onClick={() => onShowQr(`${node.city} 订阅二维码`, subUrl)}
                className="flex items-center justify-center gap-1.5 rounded-xl border border-white/10 bg-white/6 px-3.5 py-2 text-[12px] font-medium text-zinc-200 transition hover:bg-white/12"
              >
                <QrCode className="h-3.5 w-3.5 text-emerald-400" />
                扫码
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Row 3: Komari-style 3-Carrier Return Quality */}
      <CarrierQualityCard
        quality={carrierQuality}
        isProbing={isProbingCarriers}
        onProbe={onProbeCarriers}
      />

      {/* Row 4: Quick Status Footer Bar */}
      <div className="glass-card flex flex-wrap items-center justify-between gap-3 rounded-2xl px-5 py-3.5 text-[12px]">
        <div className="flex flex-wrap items-center gap-5">
          <div className="flex items-center gap-2">
            <ShieldCheck className="h-4 w-4 text-emerald-400" />
            <span className="text-zinc-400">协议</span>
            <span className="font-mono-num text-zinc-100">VLESS · Reality · Vision</span>
          </div>
          <div className="flex items-center gap-2">
            <Globe className="h-4 w-4 text-sky-400" />
            <span className="text-zinc-400">伪装</span>
            <span className="font-mono-num text-zinc-100">{node.realityTarget || "TLS 1.3 SNI"}</span>
          </div>
          <div className="flex items-center gap-2">
            <Wifi className="h-4 w-4 text-emerald-400" />
            <span className="text-zinc-400">本机</span>
            <span className="font-mono-num text-zinc-100">
              {snapshot?.localShield.mihomoConnected
                ? `${snapshot.localShield.mode.toUpperCase()} · TUN ${snapshot.localShield.tunStack ?? "mixed"}`
                : "未连接"}
            </span>
          </div>
        </div>
        {xui?.error && (
          <span className="text-[11px] text-amber-300">{xui.error}</span>
        )}
      </div>
    </div>
  );
};
