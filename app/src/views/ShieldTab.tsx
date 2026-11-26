import React from "react";
import { CheckCircle2, HelpCircle, Layers, ShieldCheck, XCircle } from "lucide-react";
import type { LocalShieldStatus } from "../types/telemetry";
import { formatSpeedInline } from "../lib/format";

interface ShieldTabProps {
  shield: LocalShieldStatus | null;
}

export const ShieldTab: React.FC<ShieldTabProps> = ({ shield }) => {
  const totalConns = Math.max(1, (shield?.proxyConnections ?? 0) + (shield?.directConnections ?? 0));
  const proxyPct = Math.round(((shield?.proxyConnections ?? 0) / totalConns) * 100);
  const directPct = 100 - proxyPct;

  const checks: Array<{
    title: string;
    state: boolean | null | undefined;
    detail: string;
  }> = [
    {
      title: "TUN 虚拟网卡",
      state: shield?.tunEnabled,
      detail: shield?.tunEnabled ? `已开启 (${shield.tunStack ?? "mixed"})` : "未开启",
    },
    {
      title: "TUN 协议栈",
      state: shield?.tunStack ? shield.tunStack.toLowerCase() === "mixed" : null,
      detail: shield?.tunStack ?? "未检测",
    },
    {
      title: "Fake-IP DNS",
      state: shield?.fakeIpEnabled,
      detail: shield?.fakeIpEnabled ? "已启用" : "未启用",
    },
    {
      title: "禁用 IPv6",
      state: shield?.ipv6Disabled,
      detail: shield?.ipv6Disabled ? "ipv6: false" : "未关闭",
    },
    {
      title: "拦截 QUIC",
      state: shield?.quicBlocked,
      detail: shield?.quicBlocked ? "REJECT UDP/443" : "未生效",
    },
    {
      title: "TCP 并发建连",
      state: shield?.tcpConcurrent,
      detail: shield?.tcpConcurrent ? "tcp-concurrent: true" : "未开启",
    },
  ];

  return (
    <div className="space-y-4">
      <div className="glass-card flex flex-wrap items-center justify-between gap-4 rounded-2xl px-5 py-4">
        <div className="flex items-center gap-2 text-[14px] font-semibold text-zinc-100">
          <ShieldCheck className="h-4 w-4 text-emerald-400" />
          本机防泄露盾
        </div>

        <div className="flex items-center gap-2.5 font-mono-num text-[12px]">
          <span
            className={`rounded-full border px-3 py-1 ${
              shield?.mihomoConnected
                ? "border-emerald-500/30 bg-emerald-500/12 text-emerald-300"
                : "border-zinc-700 bg-zinc-800/70 text-zinc-400"
            }`}
          >
            {shield?.mihomoConnected
              ? `● 已连接 (${shield.mihomoSource})`
              : "○ 未连接"}
          </span>
          <span className="rounded-full border border-white/10 bg-white/5 px-3 py-1 text-zinc-200">
            ↓ {formatSpeedInline(shield?.localDownBps ?? 0)} · ↑ {formatSpeedInline(shield?.localUpBps ?? 0)}
          </span>
        </div>
      </div>

      {/* Live Routing Split */}
      <div className="glass-card rounded-2xl p-5">
        <div className="mb-2.5 flex items-center justify-between text-[12px]">
          <span className="flex items-center gap-2 font-medium text-zinc-200">
            <Layers className="h-4 w-4 text-sky-400" />
            活跃连接（{shield?.activeConnections ?? 0}）
          </span>
          <span className="font-mono-num text-zinc-400">
            代理 {shield?.proxyConnections ?? 0} ({proxyPct}%) · 直连 {shield?.directConnections ?? 0} ({directPct}%)
          </span>
        </div>
        <div className="flex h-2.5 w-full overflow-hidden rounded-full bg-white/8">
          <div
            className="h-full bg-gradient-to-r from-emerald-500 to-teal-400 transition-all duration-500"
            style={{ width: `${proxyPct}%` }}
          />
          <div
            className="h-full bg-sky-500/70 transition-all duration-500"
            style={{ width: `${directPct}%` }}
          />
        </div>
      </div>

      {/* 6-Point Checklist */}
      <div className="grid grid-cols-3 gap-3.5">
        {checks.map((item) => (
          <div key={item.title} className="glass-card rounded-xl p-4">
            <div className="flex items-center justify-between gap-2">
              <div className="text-[13px] font-medium text-zinc-100">{item.title}</div>
              {item.state === true ? (
                <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />
              ) : item.state === false ? (
                <XCircle className="h-4 w-4 shrink-0 text-rose-400" />
              ) : (
                <HelpCircle className="h-4 w-4 shrink-0 text-zinc-500" />
              )}
            </div>
            <div className="mt-2 font-mono-num text-[11.5px] text-zinc-400">{item.detail}</div>
          </div>
        ))}
      </div>
    </div>
  );
};
