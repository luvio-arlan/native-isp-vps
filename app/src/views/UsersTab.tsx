import React from "react";
import { Copy, QrCode, Users } from "lucide-react";
import type { ClientQuota, NodeConfig } from "../types/telemetry";
import { daysUntilReset, forecastCycleUsage, formatBytes } from "../lib/format";
import { copyText } from "../lib/clipboard";
import { clientTotalBytes } from "../lib/quota";

interface UsersTabProps {
  node: NodeConfig;
  clients: ClientQuota[];
  onShowQr: (title: string, url: string) => void;
  onToast: (msg: string) => void;
}

export const UsersTab: React.FC<UsersTabProps> = ({ node, clients, onShowQr, onToast }) => {
  const copyUrl = async (url: string, label: string) => {
    onToast((await copyText(url)) ? `已复制 ${label} 的万能订阅链接` : "复制失败，请先点击窗口后重试");
  };

  return (
    <div className="space-y-4">
      <div className="glass-card flex items-center justify-between gap-3 rounded-2xl px-5 py-4">
        <div className="flex items-center gap-2 text-[14px] font-semibold text-zinc-100">
          <Users className="h-4 w-4 text-emerald-400" />
          订阅与配额
        </div>
        <div className="inline-flex items-center gap-2 rounded-xl border border-white/8 bg-black/35 px-3 py-1.5 font-mono-num text-[12px]">
          <span className="text-zinc-400">每月 {node.resetDay} 日重置</span>
          <span className="text-zinc-600">·</span>
          <span className="font-semibold text-emerald-300">
            剩余 {daysUntilReset(node.resetDay)} 天
          </span>
        </div>
      </div>

      {clients.length === 0 ? (
        <div className="glass-card rounded-2xl p-8 text-center text-[12.5px] text-zinc-400">
          暂无客户端配额数据
        </div>
      ) : (
        <div className={clients.length === 1 ? "grid max-w-xl grid-cols-1 gap-4" : "grid grid-cols-2 gap-4"}>
          {clients.map((client) => {
            const used = client.upBytes + client.downBytes;
            const total = clientTotalBytes(node, client);
            const fc = forecastCycleUsage(used, total, client.resetDay || node.resetDay);
            const subUrl = client.subUrl || node.subUrl;

            return (
              <div key={client.email} className="glass-card glass-card-hover rounded-2xl p-5">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2.5">
                    <span
                      className={`h-2.5 w-2.5 rounded-full ${
                        client.online
                          ? "bg-emerald-400 shadow-[0_0_8px_#10b981]"
                          : "bg-zinc-600"
                      }`}
                    />
                    <div>
                      <div className="text-[14px] font-semibold text-zinc-100">{client.email}</div>
                      <div className="text-[11px] text-zinc-500">
                        {client.online ? "在线" : "空闲"}
                        {!client.enable && " · 已禁用"}
                      </div>
                    </div>
                  </div>

                  <span
                    className={`rounded-full border px-2.5 py-0.5 text-[10.5px] font-medium ${
                      fc.willExceed
                        ? "border-amber-500/30 bg-amber-500/12 text-amber-300"
                        : "border-emerald-500/25 bg-emerald-500/10 text-emerald-300"
                    }`}
                  >
                    {fc.willExceed ? "预计偏紧" : "配额充裕"}
                  </span>
                </div>

                <div className="mt-4 flex items-baseline justify-between font-mono-num">
                  <span className="text-[20px] font-semibold text-zinc-100">
                    {formatBytes(used)}
                  </span>
                  <span className="text-[12px] text-zinc-400">
                    / {formatBytes(total)} ({fc.pct.toFixed(1)}%)
                  </span>
                </div>

                <div className="mt-2 h-2 w-full overflow-hidden rounded-full bg-white/8">
                  <div
                    className="h-full rounded-full bg-gradient-to-r from-emerald-500 to-sky-400 transition-all duration-500"
                    style={{ width: `${Math.max(2, fc.pct)}%` }}
                  />
                </div>

                <div className="mt-2.5 flex items-center justify-between font-mono-num text-[11px] text-zinc-500">
                  <span>↑ {formatBytes(client.upBytes)} · ↓ {formatBytes(client.downBytes)}</span>
                  <span>预计月耗 {formatBytes(fc.projectedBytes)}</span>
                </div>

                {subUrl && (
                  <div className="mt-4 flex gap-2 border-t border-white/6 pt-3.5">
                    <button
                      onClick={() => copyUrl(subUrl, client.email)}
                      className="flex flex-1 items-center justify-center gap-1.5 rounded-xl border border-white/10 bg-white/6 py-2 text-[11.5px] font-medium text-zinc-100 transition hover:bg-white/12"
                    >
                      <Copy className="h-3.5 w-3.5 text-sky-400" />
                      复制订阅
                    </button>
                    <button
                      onClick={() => onShowQr(`${client.email} · 扫码导入`, subUrl)}
                      className="flex items-center justify-center gap-1.5 rounded-xl border border-white/10 bg-white/6 px-3.5 py-2 text-[11.5px] font-medium text-zinc-200 transition hover:bg-white/12"
                    >
                      <QrCode className="h-3.5 w-3.5 text-emerald-400" />
                      扫码
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
