import React from "react";
import {
  ArrowUpCircle,
  CheckCircle2,
  Download,
  FolderSync,
  Github,
  Loader2,
  RefreshCw,
  Sliders,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AppSettings, GitHubReleaseInfo } from "../types/telemetry";
import type { UpdateStage } from "../hooks/useUpdater";

interface SettingsTabProps {
  settings: AppSettings | null;
  onSaveSettings: (patch: Partial<AppSettings>) => Promise<void>;
  updateStage: UpdateStage;
  releaseInfo: GitHubReleaseInfo | null;
  hasNativeUpdater: boolean;
  progressPct: number;
  updateError: string | null;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
}

export const SettingsTab: React.FC<SettingsTabProps> = ({
  settings,
  onSaveSettings,
  updateStage,
  releaseInfo,
  hasNativeUpdater,
  progressPct,
  updateError,
  onCheckUpdate,
  onInstallUpdate,
}) => {
  if (!settings) return null;

  return (
    <div className="space-y-4">
      {/* GitHub Releases Auto-Update Panel */}
      <div className="glass-card rounded-2xl p-5">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-[14px] font-semibold text-zinc-100">
              <Github className="h-4 w-4 text-emerald-400" />
              软件更新
            </div>
            <div className="mt-2.5 flex flex-wrap items-center gap-2.5 font-mono-num text-[11.5px]">
              <span className="rounded-lg border border-white/10 bg-black/35 px-2.5 py-1 text-zinc-200">
                当前 v{releaseInfo?.currentVersion ?? "0.1.0"}
              </span>
              {releaseInfo?.latestVersion && (
                <span className="rounded-lg border border-white/10 bg-black/35 px-2.5 py-1 text-zinc-300">
                  最新 v{releaseInfo.latestVersion}
                </span>
              )}
              {updateStage === "up-to-date" && (
                <span className="inline-flex items-center gap-1 text-emerald-300">
                  <CheckCircle2 className="h-3.5 w-3.5" />
                  已是最新
                </span>
              )}
              {updateError && <span className="text-rose-300">{updateError}</span>}
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => openUrl("https://github.com/luvio-arlan/native-isp-vps/releases")}
              className="rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-[12px] text-zinc-300 transition hover:bg-white/10"
            >
              Releases 页面
            </button>

            {updateStage === "available" || updateStage === "downloading" || updateStage === "installing" ? (
              <button
                onClick={onInstallUpdate}
                disabled={updateStage === "downloading" || updateStage === "installing"}
                className="inline-flex items-center gap-1.5 rounded-xl bg-emerald-500 px-4 py-2 text-[12px] font-medium text-zinc-950 transition hover:bg-emerald-400 disabled:opacity-60"
              >
                {updateStage === "downloading" || updateStage === "installing" ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    {updateStage === "installing" ? "正在重启安装..." : `下载中 ${progressPct}%`}
                  </>
                ) : (
                  <>
                    {hasNativeUpdater ? <Download className="h-3.5 w-3.5" /> : <ArrowUpCircle className="h-3.5 w-3.5" />}
                    {hasNativeUpdater ? "下载并自动重启更新" : "前往下载新版本"}
                  </>
                )}
              </button>
            ) : (
              <button
                onClick={onCheckUpdate}
                disabled={updateStage === "checking"}
                className="inline-flex items-center gap-1.5 rounded-xl bg-white/10 px-4 py-2 text-[12px] font-medium text-zinc-100 transition hover:bg-white/15 disabled:opacity-50"
              >
                <RefreshCw className={`h-3.5 w-3.5 ${updateStage === "checking" ? "animate-spin" : ""}`} />
                {updateStage === "checking" ? "检查中..." : "检查更新"}
              </button>
            )}
          </div>
        </div>

        {(updateStage === "downloading" || updateStage === "installing") && (
          <div className="mt-4 h-1.5 w-full overflow-hidden rounded-full bg-white/10">
            <div
              className="h-full rounded-full bg-emerald-400 transition-all duration-300"
              style={{ width: `${progressPct}%` }}
            />
          </div>
        )}
      </div>

      {/* Menu Bar & Polling */}
      <div className="glass-card rounded-2xl p-5">
        <div className="mb-4 flex items-center gap-2 text-[14px] font-semibold text-zinc-100">
          <Sliders className="h-4 w-4 text-sky-400" />
          外观与状态栏
        </div>

        <div className="mb-3.5 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-white/7 bg-black/30 px-4 py-3">
          <div className="text-[12.5px] font-medium text-zinc-100">外观主题</div>
          <div className="grid grid-cols-2 gap-1.5 rounded-xl bg-black/35 p-1">
            {[
              { id: "titanium", label: "钛合金 · 浅色" },
              { id: "obsidian", label: "曜石黑 · 暗夜" },
            ].map((opt) => (
              <button
                key={opt.id}
                onClick={() => onSaveSettings({ theme: opt.id })}
                className={`rounded-lg px-3 py-1.5 text-[11.5px] font-medium transition ${
                  (settings.theme || "titanium") === opt.id
                    ? "bg-emerald-500/20 text-emerald-200 ring-1 ring-emerald-500/40"
                    : "text-zinc-400 hover:bg-white/10"
                }`}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>

        <div className="grid grid-cols-2 gap-3.5">
          <div className="rounded-xl border border-white/7 bg-black/30 p-4">
            <div className="text-[12.5px] font-medium text-zinc-100">状态栏显示</div>
            <div className="mt-3 grid grid-cols-3 gap-1.5">
              {[
                { id: "speed-rtt", label: "网速+延迟" },
                { id: "rtt-only", label: "仅延迟" },
                { id: "dot-only", label: "极简图标" },
              ].map((opt) => (
                <button
                  key={opt.id}
                  onClick={() => onSaveSettings({ trayDisplay: opt.id })}
                  className={`rounded-lg py-1.5 text-[11.5px] font-medium transition ${
                    settings.trayDisplay === opt.id
                      ? "bg-emerald-500/20 text-emerald-200 ring-1 ring-emerald-500/40"
                      : "bg-white/5 text-zinc-400 hover:bg-white/10"
                  }`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          </div>

          <div className="rounded-xl border border-white/7 bg-black/30 p-4">
            <div className="text-[12.5px] font-medium text-zinc-100">刷新频率</div>
            <div className="mt-3 flex items-center gap-3 font-mono-num text-[12px]">
              <label className="flex items-center gap-1.5 text-zinc-300">
                <span>活跃</span>
                <select
                  value={settings.pollActiveSecs}
                  onChange={(e) => onSaveSettings({ pollActiveSecs: Number(e.target.value) })}
                  className="rounded-lg border border-white/10 bg-[#0d1018] px-2 py-1 text-zinc-100"
                >
                  <option value={1}>1s</option>
                  <option value={2}>2s</option>
                  <option value={3}>3s</option>
                  <option value={5}>5s</option>
                </select>
              </label>
              <label className="flex items-center gap-1.5 text-zinc-300">
                <span>后台</span>
                <select
                  value={settings.pollIdleSecs}
                  onChange={(e) => onSaveSettings({ pollIdleSecs: Number(e.target.value) })}
                  className="rounded-lg border border-white/10 bg-[#0d1018] px-2 py-1 text-zinc-100"
                >
                  <option value={10}>10s</option>
                  <option value={15}>15s</option>
                  <option value={30}>30s</option>
                  <option value={60}>60s</option>
                </select>
              </label>
            </div>
          </div>
        </div>

        <div className="mt-3.5 flex items-center justify-between gap-3 rounded-xl border border-white/7 bg-black/30 px-4 py-3">
          <div className="text-[12.5px] font-medium text-zinc-100">演示模式</div>
          <button
            onClick={() => onSaveSettings({ demoMode: !settings.demoMode })}
            className={`relative h-6 w-11 rounded-full transition ${
              settings.demoMode ? "bg-emerald-500" : "bg-white/15"
            }`}
          >
            <span
              className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all ${
                settings.demoMode ? "left-[22px]" : "left-0.5"
              }`}
            />
          </button>
        </div>
      </div>

      {/* Zero-Touch AI Inbox */}
      <div className="glass-card rounded-2xl p-5">
        <div className="flex items-center gap-2 text-[13px] font-semibold text-zinc-100">
          <FolderSync className="h-4 w-4 text-emerald-400" />
          自动导入目录
        </div>
        <div className="mt-2.5 select-text rounded-xl border border-white/8 bg-black/40 px-3.5 py-2.5 font-mono-num text-[11.5px] text-zinc-300">
          {settings.inboxDir}
        </div>
      </div>
    </div>
  );
};
