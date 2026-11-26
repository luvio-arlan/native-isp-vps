import React, { useCallback, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Activity,
  Gauge,
  Minimize2,
  Moon,
  Plus,
  Settings,
  ShieldCheck,
  Sun,
  Terminal,
  Trash2,
  Users,
} from "lucide-react";
import { useTelemetry } from "../hooks/useTelemetry";
import { useUpdater } from "../hooks/useUpdater";
import { StatusBadge } from "../components/StatusBadge";
import { AddNodeModal } from "../components/AddNodeModal";
import { QrModal } from "../components/QrModal";
import { UpdateBanner } from "../components/UpdateBanner";
import { OverviewTab } from "./OverviewTab";
import { NetworkTab } from "./NetworkTab";
import { UsersTab } from "./UsersTab";
import { AuditTab } from "./AuditTab";
import { ShieldTab } from "./ShieldTab";
import { SettingsTab } from "./SettingsTab";
import { api, isTauriRuntime } from "../lib/tauri";

type NavKey = "overview" | "network" | "users" | "audit" | "shield" | "settings";

const NAV_ITEMS: Array<{ id: NavKey; label: string; icon: React.ComponentType<{ className?: string }> }> = [
  { id: "overview", label: "总览驾驶舱", icon: Gauge },
  { id: "network", label: "链路与 AI 测速", icon: Activity },
  { id: "users", label: "用户与订阅", icon: Users },
  { id: "audit", label: "内核深度体检", icon: Terminal },
  { id: "shield", label: "本机防泄露盾", icon: ShieldCheck },
  { id: "settings", label: "设置与更新", icon: Settings },
];

export const CockpitApp: React.FC = () => {
  const initialTab = (() => {
    const t = new URLSearchParams(window.location.search).get("tab") as NavKey | null;
    return t && NAV_ITEMS.some((i) => i.id === t) ? t : "overview";
  })();
  const [nav, setNav] = useState<NavKey>(initialTab);
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [qrState, setQrState] = useState<{ title: string; url: string } | null>(null);

  const handleDragMouseDown = useCallback((e: React.MouseEvent<HTMLElement>) => {
    if (e.button !== 0 || e.detail >= 2 || !isTauriRuntime()) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("button, input, textarea, select, a, [data-no-drag]")) return;
    e.preventDefault();
    getCurrentWindow().startDragging().catch(() => {});
  }, []);

  const handleTitlebarDoubleClick = useCallback((e: React.MouseEvent<HTMLElement>) => {
    if (!isTauriRuntime()) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("button, input, textarea, select, a, [data-no-drag]")) return;
    getCurrentWindow().toggleMaximize().catch(() => {});
  }, []);

  const {
    nodes,
    activeNode,
    isDemo,
    theme,
    snapshot,
    settings,
    aiProbes,
    leakResult,
    auditReport,
    history24h,
    isProbing,
    isAuditing,
    toast,
    showToast,
    selectNode,
    addNode,
    removeNode,
    loadHistory24h,
    runFullProbe,
    runAudit,
    saveSettings,
  } = useTelemetry();

  const updater = useUpdater(settings?.autoCheckUpdates ?? true);

  return (
    <div className="cockpit-shell flex h-screen w-screen overflow-hidden">
      {/* Left Fleet & Navigation Rail */}
      <aside className="cockpit-rail flex w-[238px] shrink-0 flex-col">
        {/* macOS Overlay Traffic Light Safe Drag Area */}
        <div
          data-tauri-drag-region
          onMouseDown={handleDragMouseDown}
          onDoubleClick={handleTitlebarDoubleClick}
          className="titlebar-drag flex h-12 items-center justify-end px-4 pt-2"
        >
          <span className="pointer-events-none rounded-md border border-white/8 bg-white/[0.04] px-2 py-0.5 font-mono-num text-[10px] tracking-wider text-zinc-400">
            v{updater.releaseInfo?.currentVersion ?? "0.1.1"}
          </span>
        </div>

        {/* Brand */}
        <div
          data-tauri-drag-region
          onMouseDown={handleDragMouseDown}
          onDoubleClick={handleTitlebarDoubleClick}
          className="titlebar-drag px-4 pb-3 pt-1"
        >
          <div className="pointer-events-none flex items-center gap-2.5">
            <img
              src="/brand-icon.png"
              alt="Native ISP"
              className="h-8 w-8 rounded-[9px] shadow-md"
            />
            <div className="text-[14px] font-semibold tracking-tight text-zinc-100">
              Native ISP
            </div>
          </div>
        </div>

        {/* Fleet Nodes Section */}
        <div className="px-3 py-2">
          <div className="mb-1.5 flex items-center justify-between px-1.5">
            <span className="text-[10.5px] font-medium uppercase tracking-wider text-zinc-500">
              VPS 节点舰队
            </span>
            <button
              onClick={() => setAddModalOpen(true)}
              title="接入新节点"
              className="rounded-md p-1 text-zinc-400 transition hover:bg-white/10 hover:text-emerald-300"
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          </div>

          <div className="max-h-56 space-y-1 overflow-y-auto pr-0.5">
            {nodes.length === 0 ? (
              <div className="rounded-xl border border-sky-500/25 bg-sky-500/[0.08] px-3 py-2.5">
                <div className="flex items-center gap-1.5 text-[12px] font-medium text-sky-200">
                  <span>{activeNode.flag}</span>
                  <span className="truncate">{activeNode.city}</span>
                </div>
                <div className="mt-0.5 text-[10.5px] text-sky-300/75">
                  演示节点
                </div>
              </div>
            ) : (
              nodes.map((n) => {
                const selected = !isDemo && n.id === activeNode.id;
                return (
                  <div
                    key={n.id}
                    onClick={() => selectNode(n.id)}
                    className={`group flex cursor-pointer items-center justify-between rounded-xl px-2.5 py-2 transition ${
                      selected
                        ? "border border-emerald-500/30 bg-emerald-500/[0.11] text-zinc-100"
                        : "border border-transparent text-zinc-400 hover:bg-white/[0.05] hover:text-zinc-200"
                    }`}
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5 text-[12px] font-medium">
                        <span>{n.flag}</span>
                        <span className="truncate">{n.city}</span>
                      </div>
                      <div className="mt-0.5 truncate font-mono-num text-[10.5px] text-zinc-500">
                        {n.host}
                      </div>
                    </div>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        if (window.confirm(`确定移除节点「${n.name}」并清理本地凭据？`)) {
                          removeNode(n.id);
                        }
                      }}
                      title="移除节点"
                      className="rounded-md p-1 text-zinc-600 opacity-0 transition hover:bg-rose-500/15 hover:text-rose-300 group-hover:opacity-100"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* Navigation Links */}
        <nav className="mt-2 flex-1 space-y-0.5 px-3">
          <div className="mb-1.5 px-1.5 text-[10.5px] font-medium uppercase tracking-wider text-zinc-500">
            控制台视图
          </div>
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const active = nav === item.id;
            return (
              <button
                key={item.id}
                onClick={() => setNav(item.id)}
                className={`flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-[12.5px] font-medium transition ${
                  active
                    ? "bg-white/10 text-zinc-50 shadow-sm"
                    : "text-zinc-400 hover:bg-white/[0.04] hover:text-zinc-200"
                }`}
              >
                <Icon className={`h-4 w-4 ${active ? "text-emerald-400" : "text-zinc-500"}`} />
                {item.label}
              </button>
            );
          })}
        </nav>

        {/* Bottom Collapse to Menu Bar Button */}
        <div className="border-t border-white/[0.07] p-3">
          <button
            onClick={() => api.collapseToTray()}
            className="flex w-full items-center justify-center gap-2 rounded-xl border border-white/8 bg-white/[0.04] py-2 text-[12px] font-medium text-zinc-300 transition hover:border-white/15 hover:bg-white/[0.08] hover:text-zinc-100"
          >
            <Minimize2 className="h-3.5 w-3.5 text-emerald-400" />
            收缩到状态栏常驻
          </button>
        </div>
      </aside>

      {/* Main Telemetry Workspace */}
      <main className="flex min-w-0 flex-1 flex-col">
        {/* Top Draggable Header Bar */}
        <header
          data-tauri-drag-region
          onMouseDown={handleDragMouseDown}
          onDoubleClick={handleTitlebarDoubleClick}
          className="cockpit-header titlebar-drag flex h-14 shrink-0 items-center justify-between px-6"
        >
          <div className="pointer-events-none flex items-center gap-3">
            <h1 className="text-[15px] font-semibold tracking-tight text-zinc-100">
              {activeNode.name}
            </h1>
            <StatusBadge
              status={snapshot?.healthStatus ?? "demo"}
              label={
                isDemo
                  ? "演示数据"
                  : snapshot?.healthStatus === "optimal"
                    ? "链路极佳"
                    : snapshot?.healthStatus === "degraded"
                      ? "轻微波动"
                      : "需要关注"
              }
            />
          </div>

          <div data-no-drag className="titlebar-nodrag flex items-center gap-2">
            <UpdateBanner
              stage={updater.stage}
              releaseInfo={updater.releaseInfo}
              hasNativeUpdater={updater.hasNativeUpdater}
              progressPct={updater.progressPct}
              onInstall={updater.installUpdate}
            />
            <button
              onClick={() => saveSettings({ theme: theme === "titanium" ? "obsidian" : "titanium" })}
              title={theme === "titanium" ? "切换到曜石暗黑皮肤" : "切换到原色钛合金浅色皮肤"}
              className="inline-flex h-8 items-center gap-1.5 rounded-xl border border-white/8 bg-white/[0.04] px-2.5 text-[11.5px] font-medium text-zinc-300 transition hover:bg-white/10 hover:text-zinc-100"
            >
              {theme === "titanium" ? (
                <>
                  <Sun className="h-3.5 w-3.5 text-amber-500" />
                  <span>钛合金</span>
                </>
              ) : (
                <>
                  <Moon className="h-3.5 w-3.5 text-sky-400" />
                  <span>曜石黑</span>
                </>
              )}
            </button>
            <button
              onClick={() => setAddModalOpen(true)}
              className="inline-flex items-center gap-1.5 rounded-xl bg-emerald-500 px-3 py-1.5 text-[12px] font-medium text-zinc-950 transition hover:bg-emerald-400"
            >
              <Plus className="h-3.5 w-3.5" />
              接入节点
            </button>
          </div>
        </header>

        {/* Active View Stage */}
        <div className="flex-1 overflow-y-auto px-6 py-5">
          {nav === "overview" && (
            <OverviewTab
              node={activeNode}
              snapshot={snapshot}
              onShowQr={(title, url) => setQrState({ title, url })}
              onToast={showToast}
            />
          )}
          {nav === "network" && (
            <NetworkTab
              node={activeNode}
              snapshot={snapshot}
              history24h={history24h}
              aiProbes={aiProbes}
              leakResult={leakResult}
              isProbing={isProbing}
              onRunProbe={runFullProbe}
              onLoadHistory24h={loadHistory24h}
            />
          )}
          {nav === "users" && (
            <UsersTab
              node={activeNode}
              clients={snapshot?.clients ?? []}
              onShowQr={(title, url) => setQrState({ title, url })}
              onToast={showToast}
            />
          )}
          {nav === "audit" && (
            <AuditTab
              node={activeNode}
              snapshot={snapshot}
              report={auditReport}
              isAuditing={isAuditing}
              onRunAudit={runAudit}
              onToast={showToast}
            />
          )}
          {nav === "shield" && <ShieldTab shield={snapshot?.localShield ?? null} />}
          {nav === "settings" && (
            <SettingsTab
              settings={settings}
              onSaveSettings={saveSettings}
              updateStage={updater.stage}
              releaseInfo={updater.releaseInfo}
              hasNativeUpdater={updater.hasNativeUpdater}
              progressPct={updater.progressPct}
              updateError={updater.errorMsg}
              onCheckUpdate={() => updater.checkForUpdates(false)}
              onInstallUpdate={updater.installUpdate}
            />
          )}
        </div>
      </main>

      {/* Modals */}
      <AddNodeModal
        open={addModalOpen}
        inboxDir={settings?.inboxDir ?? "~/.local/share/native-isp-vps/inbox"}
        onClose={() => setAddModalOpen(false)}
        onSubmit={async (payload) => {
          await addNode(payload);
        }}
      />

      <QrModal
        title={qrState?.title ?? ""}
        url={qrState?.url ?? null}
        onClose={() => setQrState(null)}
      />

      {/* Toast Notification */}
      {toast && (
        <div className="fixed bottom-5 right-5 z-50 rounded-xl border border-emerald-500/30 bg-[#0d121a]/95 px-4 py-2.5 text-[12px] font-medium text-emerald-200 shadow-2xl backdrop-blur-md">
          {toast}
        </div>
      )}
    </div>
  );
};
