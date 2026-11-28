import React, { useState } from "react";
import {
  AlertTriangle,
  Bot,
  Check,
  CheckCircle2,
  RefreshCw,
  Shield,
  Terminal,
  XCircle,
} from "lucide-react";
import type { AuditReport, NodeConfig, TelemetrySnapshot } from "../types/telemetry";
import { GaugeRing } from "../components/GaugeRing";
import { buildAiRepairPrompt } from "../lib/diagnostic";
import { copyText } from "../lib/clipboard";

interface AuditTabProps {
  node: NodeConfig;
  snapshot: TelemetrySnapshot | null;
  report: AuditReport | null;
  isAuditing: boolean;
  onRunAudit: () => void;
  onToast: (msg: string) => void;
}

export const AuditTab: React.FC<AuditTabProps> = ({
  node,
  snapshot,
  report,
  isAuditing,
  onRunAudit,
  onToast,
}) => {
  const [copiedPrompt, setCopiedPrompt] = useState(false);

  const handleCopyRepair = async () => {
    const prompt = buildAiRepairPrompt(node, snapshot, report);
    if (!(await copyText(prompt))) {
      onToast("复制失败，请先点击窗口后重试");
      return;
    }
    setCopiedPrompt(true);
    onToast("已复制脱敏 AI 诊断修复包，直接粘贴给 AI 即可自动修复");
    setTimeout(() => setCopiedPrompt(false), 2000);
  };

  return (
    <div className="space-y-4">
      <div className="glass-card flex flex-wrap items-center justify-between gap-4 rounded-2xl p-5">
        <div className="flex items-center gap-4">
          <GaugeRing
            value={report ? report.score : 0}
            label="内核合规分"
            size={84}
            invertColor
          />
          <div>
            <h3 className="text-[15px] font-semibold text-zinc-100">
              内核合规体检
            </h3>
            {report?.fail2banBannedTotal != null && (
              <div className="mt-2 inline-flex items-center gap-1.5 rounded-full border border-emerald-500/25 bg-emerald-500/10 px-2.5 py-0.5 font-mono-num text-[11px] text-emerald-300">
                <Shield className="h-3 w-3" />
                Fail2ban 拦截 {report.fail2banBannedTotal} 次 · 封禁{" "}
                {report.fail2banBannedCurrent ?? 0} IP
              </div>
            )}
          </div>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={handleCopyRepair}
            className="inline-flex items-center gap-1.5 rounded-xl border border-white/10 bg-white/6 px-3.5 py-2 text-[12px] font-medium text-zinc-200 transition hover:bg-white/12"
          >
            {copiedPrompt ? (
              <Check className="h-3.5 w-3.5 text-emerald-400" />
            ) : (
              <Bot className="h-3.5 w-3.5 text-sky-400" />
            )}
            {copiedPrompt ? "已复制" : "复制诊断包"}
          </button>

          <button
            onClick={onRunAudit}
            disabled={isAuditing}
            className="inline-flex items-center gap-1.5 rounded-xl bg-emerald-500 px-4 py-2 text-[12px] font-medium text-zinc-950 transition hover:bg-emerald-400 disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isAuditing ? "animate-spin" : ""}`} />
            {isAuditing ? "体检中..." : "开始体检"}
          </button>
        </div>
      </div>

      {report?.error && (
        <div className="flex items-start gap-2.5 rounded-2xl border border-amber-500/25 bg-amber-500/10 p-4 text-[12px] text-amber-200">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
          <div>
            <div className="font-medium">SSH 深度体检未完成</div>
            <div className="mt-1 font-mono-num text-[11.5px] text-amber-200/80">{report.error}</div>
          </div>
        </div>
      )}

      {!report ? (
        <div className="glass-card rounded-2xl p-10 text-center">
          <Terminal className="mx-auto h-7 w-7 text-zinc-500" />
          <p className="mt-3 text-[13px] text-zinc-300">尚未执行内核合规体检</p>
        </div>
      ) : (
        <div className="grid grid-cols-3 gap-3">
          {report.items.map((item) => (
            <div
              key={item.key}
              className={`glass-card rounded-xl p-4 ${
                item.passed ? "border-white/8" : "border-rose-500/35 bg-rose-500/[0.06]"
              }`}
            >
              <div className="flex items-start justify-between gap-2">
                <span className="text-[12.5px] font-medium text-zinc-100">{item.label}</span>
                {item.passed ? (
                  <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-400" />
                ) : (
                  <XCircle className="h-4 w-4 shrink-0 text-rose-400" />
                )}
              </div>
              <div className="mt-2.5 font-mono-num text-[12px] text-zinc-200">
                当前：{item.actual || "--"}
              </div>
              <div className="mt-0.5 font-mono-num text-[10.5px] text-zinc-500">
                基线：{item.expected}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
