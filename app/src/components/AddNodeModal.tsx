import React, { useEffect, useState } from "react";
import { Bot, Check, Copy, FileJson, KeyRound, Server, Sparkles, X } from "lucide-react";
import type { HandoffPayload } from "../types/telemetry";
import { buildAiDeployPrompt } from "../lib/diagnostic";
import { AnimatedModal } from "./AnimatedModal";
import { copyText } from "../lib/clipboard";

interface AddNodeModalProps {
  open: boolean;
  inboxDir: string;
  onClose: () => void;
  onSubmit: (payload: HandoffPayload) => Promise<void>;
}

export const AddNodeModal: React.FC<AddNodeModalProps> = ({
  open,
  inboxDir,
  onClose,
  onSubmit,
}) => {
  const [tab, setTab] = useState<"ai" | "manual" | "json">("ai");
  const [copiedPrompt, setCopiedPrompt] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Manual form state
  const [host, setHost] = useState("");
  const [city, setCity] = useState("");
  const [flag, setFlag] = useState("");
  const [ispTag, setIspTag] = useState("Dual ISP");
  const [panelUrl, setPanelUrl] = useState("");
  const [apiToken, setApiToken] = useState("");
  const [panelUsername, setPanelUsername] = useState("");
  const [panelPassword, setPanelPassword] = useState("");
  const [subUrl, setSubUrl] = useState("");
  const [monthlyTotalGb, setMonthlyTotalGb] = useState("2000");
  const [resetDay, setResetDay] = useState("25");
  const [sshKeyPath, setSshKeyPath] = useState("~/.ssh/id_ed25519_native_isp");

  // JSON import
  const [rawJson, setRawJson] = useState("");

  useEffect(() => {
    if (!open) return;
    setError(null);
  }, [open]);

  const deployPrompt = buildAiDeployPrompt();

  const handleCopyPrompt = async () => {
    if (!(await copyText(deployPrompt))) {
      setError("复制失败，请手动选中文本复制");
      return;
    }
    setCopiedPrompt(true);
    setTimeout(() => setCopiedPrompt(false), 2000);
  };

  const handleManualSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!host.trim() || !panelUrl.trim()) {
      setError("请至少填写 VPS 公网 IP 与 3x-ui 面板地址");
      return;
    }
    setSubmitting(true);
    try {
      await onSubmit({
        host: host.trim(),
        city: city.trim() || host.trim(),
        flag: flag.trim() || "🌐",
        ispTag: ispTag.trim() || "Dual ISP",
        panelUrl: panelUrl.trim(),
        apiToken: apiToken.trim() || undefined,
        panelUsername: panelUsername.trim() || undefined,
        panelPassword: panelPassword.trim() || undefined,
        subUrl: subUrl.trim() || undefined,
        monthlyTotalGb: Number(monthlyTotalGb) || 2000,
        resetDay: Number(resetDay) || 1,
        sshKeyPath: sshKeyPath.trim() || undefined,
        source: "manual",
      });
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const handleJsonSubmit = async () => {
    setError(null);
    try {
      const parsed = JSON.parse(rawJson) as HandoffPayload;
      if (!parsed.host || !parsed.panelUrl) {
        setError("JSON 需至少包含 host 与 panelUrl 字段");
        return;
      }
      setSubmitting(true);
      await onSubmit({ ...parsed, source: parsed.source ?? "ai-handoff" });
      onClose();
    } catch (err) {
      setError(`JSON 解析或保存失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <AnimatedModal open={open} onClose={onClose} panelClassName="max-w-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-[16px] font-semibold text-zinc-100">接入 VPS 节点</h2>
          <button
            onClick={onClose}
            aria-label="关闭"
            className="rounded-lg p-1.5 text-zinc-400 transition hover:bg-white/8 hover:text-zinc-100"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Mode Switcher */}
        <div className="mb-4 grid grid-cols-3 gap-1.5 rounded-xl bg-black/40 p-1">
          {[
            { id: "ai", label: "AI 自动纳管", icon: Sparkles },
            { id: "manual", label: "连接已有 3x-ui", icon: Server },
            { id: "json", label: "粘贴交接 JSON", icon: FileJson },
          ].map((item) => {
            const Icon = item.icon;
            const active = tab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => {
                  setTab(item.id as "ai" | "manual" | "json");
                  setError(null);
                }}
                className={`flex items-center justify-center gap-1.5 rounded-lg py-2 text-[12px] font-medium transition ${
                  active
                    ? "bg-white/10 text-zinc-100 shadow-sm"
                    : "text-zinc-400 hover:text-zinc-200"
                }`}
              >
                <Icon className="h-3.5 w-3.5" />
                {item.label}
              </button>
            );
          })}
        </div>

        {tab === "ai" && (
          <div key="tab-ai" className="animate-modal-tab space-y-3.5">
            <div className="rounded-xl border border-emerald-500/20 bg-emerald-500/[0.06] p-3.5">
              <div className="flex items-center gap-2 text-[12px] font-medium text-emerald-300">
                <Bot className="h-4 w-4" />
                Zero-Touch 自动收件箱正在后台实时监听
              </div>
              <p className="mt-1.5 text-[12px] leading-relaxed text-zinc-300">
                把下面指令发给装有 <code className="rounded bg-white/8 px-1 py-0.5 text-emerald-300">native-isp-vps</code> Skill 的 AI Agent（Antigravity / Claude Code / Cursor），部署完成后会自动将节点加密接入此看板并弹出系统通知。
              </p>
              <div className="mt-2.5 rounded-lg bg-black/45 px-3 py-2 font-mono-num text-[11px] text-zinc-400">
                监听路径：{inboxDir}
              </div>
            </div>

            <div className="rounded-xl border border-white/8 bg-black/35 p-3">
              <div className="mb-1.5 flex items-center justify-between">
                <span className="text-[11px] font-medium text-zinc-400">一键发给 AI 的标准部署指令</span>
                <button
                  onClick={handleCopyPrompt}
                  className="inline-flex items-center gap-1 rounded-md bg-emerald-500/15 px-2.5 py-1 text-[11px] font-medium text-emerald-300 transition hover:bg-emerald-500/25"
                >
                  {copiedPrompt ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
                  {copiedPrompt ? "已复制" : "复制指令"}
                </button>
              </div>
              <p className="select-text text-[12px] leading-relaxed text-zinc-200">{deployPrompt}</p>
            </div>
          </div>
        )}

        {tab === "manual" && (
          <form key="tab-manual" onSubmit={handleManualSubmit} className="animate-modal-tab space-y-3">
            <div className="grid grid-cols-3 gap-2.5">
              <label className="col-span-2 block">
                <span className="mb-1 block text-[11px] text-zinc-400">VPS 公网 IPv4 *</span>
                <input
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                  placeholder="例如 38.55.142.88"
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 font-mono-num text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-[11px] text-zinc-400">城市 / 标签</span>
                <div className="flex gap-1.5">
                  <input
                    value={flag}
                    onChange={(e) => setFlag(e.target.value)}
                    placeholder="🌐"
                    aria-label="国旗 Emoji"
                    className="w-10 rounded-lg border border-white/10 bg-black/40 px-2 py-2 text-center text-[12px] outline-none"
                  />
                  <input
                    value={city}
                    onChange={(e) => setCity(e.target.value)}
                    placeholder="Los Angeles"
                    className="w-full rounded-lg border border-white/10 bg-black/40 px-2.5 py-2 text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
                  />
                </div>
              </label>
            </div>

            <label className="block">
              <span className="mb-1 block text-[11px] text-zinc-400">3x-ui 面板地址 *</span>
              <input
                value={panelUrl}
                onChange={(e) => setPanelUrl(e.target.value)}
                placeholder="http://38.55.142.88:2053/yourpath"
                className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 font-mono-num text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
              />
            </label>

            <div className="grid grid-cols-2 gap-2.5">
              <label className="block">
                <span className="mb-1 flex items-center gap-1 text-[11px] text-zinc-400">
                  <KeyRound className="h-3 w-3 text-emerald-400" />
                  3x-ui API Token（推荐）
                </span>
                <input
                  type="password"
                  value={apiToken}
                  onChange={(e) => setApiToken(e.target.value)}
                  placeholder="Bearer Token（可选）"
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 font-mono-num text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-[11px] text-zinc-400">线路属性标签</span>
                <input
                  value={ispTag}
                  onChange={(e) => setIspTag(e.target.value)}
                  placeholder="Dual ISP / CN2 GIA"
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
                />
              </label>
            </div>

            <div className="grid grid-cols-2 gap-2.5">
              <label className="block">
                <span className="mb-1 block text-[11px] text-zinc-400">面板用户名</span>
                <input
                  value={panelUsername}
                  onChange={(e) => setPanelUsername(e.target.value)}
                  placeholder="admin"
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-[11px] text-zinc-400">面板密码</span>
                <input
                  type="password"
                  value={panelPassword}
                  onChange={(e) => setPanelPassword(e.target.value)}
                  placeholder="••••••••"
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
                />
              </label>
            </div>

            <label className="block">
              <span className="mb-1 block text-[11px] text-zinc-400">订阅链接</span>
              <input
                value={subUrl}
                onChange={(e) => setSubUrl(e.target.value)}
                placeholder="http://38.55.142.88:2096/sub/xxxxxxxx"
                className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 font-mono-num text-[12px] text-zinc-100 outline-none focus:border-emerald-500/50"
              />
            </label>

            <div className="grid grid-cols-3 gap-2.5">
              <label className="block">
                <span className="mb-1 block text-[11px] text-zinc-400">月总流量 (GB)</span>
                <input
                  value={monthlyTotalGb}
                  onChange={(e) => setMonthlyTotalGb(e.target.value)}
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 font-mono-num text-[12px] text-zinc-100 outline-none"
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-[11px] text-zinc-400">每月重置日</span>
                <input
                  value={resetDay}
                  onChange={(e) => setResetDay(e.target.value)}
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 font-mono-num text-[12px] text-zinc-100 outline-none"
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-[11px] text-zinc-400">SSH 私钥路径</span>
                <input
                  value={sshKeyPath}
                  onChange={(e) => setSshKeyPath(e.target.value)}
                  className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2 font-mono-num text-[12px] text-zinc-100 outline-none"
                />
              </label>
            </div>

            {error && (
              <div className="rounded-lg border border-rose-500/25 bg-rose-500/10 px-3 py-2 text-[12px] text-rose-300">
                {error}
              </div>
            )}

            <div className="flex justify-end gap-2 pt-1">
              <button
                type="button"
                onClick={onClose}
                className="rounded-lg border border-white/10 px-4 py-2 text-[12px] text-zinc-300 hover:bg-white/6"
              >
                取消
              </button>
              <button
                type="submit"
                disabled={submitting}
                className="rounded-lg bg-emerald-500 px-4 py-2 text-[12px] font-medium text-zinc-950 transition hover:bg-emerald-400 disabled:opacity-50"
              >
                {submitting ? "加密接入中..." : "加密保存并接入"}
              </button>
            </div>
          </form>
        )}

        {tab === "json" && (
          <div key="tab-json" className="animate-modal-tab space-y-3">
            <textarea
              rows={8}
              value={rawJson}
              onChange={(e) => setRawJson(e.target.value)}
              placeholder={`{\n  "host": "38.55.142.88",\n  "flag": "🇺🇸",\n  "city": "Los Angeles",\n  "ispTag": "Dual ISP",\n  "panelUrl": "http://38.55.142.88:2053/webbase",\n  "apiToken": "...",\n  "subUrl": "http://38.55.142.88:2096/sub/...",\n  "monthlyTotalGb": 2000,\n  "resetDay": 25\n}`}
              className="w-full rounded-xl border border-white/10 bg-black/45 p-3 font-mono-num text-[11.5px] text-zinc-200 outline-none focus:border-emerald-500/50"
            />
            {error && (
              <div className="rounded-lg border border-rose-500/25 bg-rose-500/10 px-3 py-2 text-[12px] text-rose-300">
                {error}
              </div>
            )}
            <div className="flex justify-end gap-2">
              <button
                onClick={onClose}
                className="rounded-lg border border-white/10 px-4 py-2 text-[12px] text-zinc-300 hover:bg-white/6"
              >
                取消
              </button>
              <button
                onClick={handleJsonSubmit}
                disabled={submitting || !rawJson.trim()}
                className="rounded-lg bg-emerald-500 px-4 py-2 text-[12px] font-medium text-zinc-950 hover:bg-emerald-400 disabled:opacity-50"
              >
                导入并保存凭据
              </button>
            </div>
          </div>
        )}
    </AnimatedModal>
  );
};
