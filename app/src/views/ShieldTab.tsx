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
    desc: string;
    state: boolean | null | undefined;
    detail: string;
  }> = [
    {
      title: "TUN 虚拟网卡模式",
      desc: "全流量接管终端 CLI、AI 工具与桌面应用，无需逐一配代理环境变量",
      state: shield?.tunEnabled,
      detail: shield?.tunEnabled ? `已开启 (${shield.tunStack ?? "mixed"})` : "未开启",
    },
    {
      title: "TUN 协议栈 Mixed（禁 mips）",
      desc: "macOS 下 Mixed 栈兼容性与吞吐最佳，避免系统高负载卡死",
      state: shield?.tunStack ? shield.tunStack.toLowerCase() === "mixed" : null,
      detail: shield?.tunStack ?? "未检测",
    },
    {
      title: "Fake-IP DoH 防污染",
      desc: "消除本地 DNS 泄漏与解析抖动，秒级命中分流规则",
      state: shield?.fakeIpEnabled,
      detail: shield?.fakeIpEnabled ? "Fake-IP 已启用" : "未检测到 Fake-IP",
    },
    {
      title: "严格禁用 IPv6",
      desc: "杜绝国内宽带原生 IPv6 绕过代理直连海外 AI 服务触发风控",
      state: shield?.ipv6Disabled,
      detail: shield?.ipv6Disabled ? "ipv6: false" : "IPv6 未关闭",
    },
    {
      title: "拦截代理内 QUIC UDP/443",
      desc: "强制 Chrome / 客户端回退纯 TCP TLS，彻底消灭白天 UDP QoS 断流",
      state: shield?.quicBlocked,
      detail: shield?.quicBlocked ? "REJECT UDP/443 生效" : "未检测到禁 QUIC 规则",
    },
    {
      title: "TCP 并发建连（Merge 加固）",
      desc: "多 IP 并发握手择优，配合 15s 保活显著提升 AI 对话首字速度",
      state: shield?.tcpConcurrent,
      detail: shield?.tcpConcurrent ? "tcp-concurrent: true" : "未开启并发建连",
    },
  ];

  return (
    <div className="space-y-4">
      <div className="glass-card flex flex-wrap items-center justify-between gap-4 rounded-2xl p-5">
        <div>
          <div className="flex items-center gap-2 text-[14px] font-semibold text-zinc-100">
            <ShieldCheck className="h-4 w-4 text-emerald-400" />
            本机 Clash Verge Rev / Mihomo 防护透视
          </div>
          <p className="mt-1 text-[12px] text-zinc-400">
            直接通过本地 Unix Socket 零延迟读取运行态，不消耗任何 VPS 资源。
          </p>
        </div>

        <div className="flex items-center gap-3 font-mono-num text-[12px]">
          <span
            className={`rounded-full border px-3 py-1 ${
              shield?.mihomoConnected
                ? "border-emerald-500/30 bg-emerald-500/12 text-emerald-300"
                : "border-zinc-700 bg-zinc-800/70 text-zinc-400"
            }`}
          >
            {shield?.mihomoConnected
              ? `● 已连接 (${shield.mihomoSource})`
              : "○ 未检测到 Clash Verge"}
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
            活跃连接分流占比（共 {shield?.activeConnections ?? 0} 条连接）
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
      <div className="grid grid-cols-2 gap-3.5">
        {checks.map((item) => (
          <div key={item.title} className="glass-card rounded-xl p-4">
            <div className="flex items-start justify-between gap-2">
              <div className="text-[13px] font-medium text-zinc-100">{item.title}</div>
              {item.state === true ? (
                <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />
              ) : item.state === false ? (
                <XCircle className="h-4 w-4 shrink-0 text-rose-400" />
              ) : (
                <HelpCircle className="h-4 w-4 shrink-0 text-zinc-500" />
              )}
            </div>
            <p className="mt-1 text-[11.5px] leading-relaxed text-zinc-400">{item.desc}</p>
            <div className="mt-2.5 font-mono-num text-[11px] text-zinc-300">{item.detail}</div>
          </div>
        ))}
      </div>
    </div>
  );
};
