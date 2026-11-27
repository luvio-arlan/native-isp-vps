import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AiProbeResult,
  AppSettings,
  AuditReport,
  CachedDiagnostics,
  CarrierQualitySnapshot,
  GitHubReleaseInfo,
  HandoffPayload,
  HistoryBucket,
  LeakCheckResult,
  NodeConfig,
  TelemetrySnapshot,
} from "../types/telemetry";

export const isTauriRuntime = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  listNodes: (): Promise<NodeConfig[]> => invoke("list_nodes"),
  getActiveNodeId: (): Promise<string | null> => invoke("get_active_node_id"),
  setActiveNode: (nodeId: string): Promise<void> => invoke("set_active_node", { nodeId }),
  upsertNode: (payload: HandoffPayload): Promise<NodeConfig> => invoke("upsert_node", { payload }),
  deleteNode: (nodeId: string): Promise<NodeConfig[]> => invoke("delete_node", { nodeId }),
  getLatestTelemetry: (): Promise<TelemetrySnapshot | null> => invoke("get_latest_telemetry"),
  getCachedDiagnostics: (): Promise<CachedDiagnostics> => invoke("get_cached_diagnostics"),
  getHistory24h: (nodeId?: string): Promise<HistoryBucket[]> =>
    invoke("get_history_24h", { nodeId: nodeId ?? null }),
  triggerRefresh: (): Promise<TelemetrySnapshot | null> => invoke("trigger_refresh"),
  probeAiEndpoints: (): Promise<AiProbeResult[]> => invoke("probe_ai_endpoints_cmd"),
  probeCarrierQuality: (): Promise<CarrierQualitySnapshot | null> =>
    invoke("probe_carrier_quality_cmd"),
  runLeakCheck: (): Promise<LeakCheckResult> => invoke("run_leak_check_cmd"),
  runSshAudit: (nodeId?: string): Promise<AuditReport> =>
    invoke("run_ssh_audit_cmd", { nodeId: nodeId ?? null }),
  getSettings: (): Promise<AppSettings> => invoke("get_settings"),
  updateSettings: (newSettings: AppSettings): Promise<AppSettings> =>
    invoke("update_settings", { newSettings }),
  checkGitHubRelease: (): Promise<GitHubReleaseInfo> => invoke("check_github_release_cmd"),
  openMainWindow: (): Promise<void> => invoke("open_main_window"),
  collapseToTray: (): Promise<void> => invoke("collapse_to_tray"),
  hidePopover: (): Promise<void> => invoke("hide_popover"),
};

export async function subscribeEvent<T>(
  event: string,
  handler: (payload: T) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen<T>(event, (ev) => handler(ev.payload));
}
