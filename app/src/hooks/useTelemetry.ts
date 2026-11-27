import { useCallback, useEffect, useRef, useState } from "react";
import { api, isTauriRuntime, subscribeEvent } from "../lib/tauri";
import type {
  AiProbeResult,
  AppSettings,
  AuditReport,
  CarrierQualitySnapshot,
  HandoffPayload,
  HistoryBucket,
  LeakCheckResult,
  NodeConfig,
  TelemetrySnapshot,
} from "../types/telemetry";

const DEMO_NODE: NodeConfig = {
  id: "demo-la",
  name: "🇺🇸 Los Angeles · Dual ISP",
  flag: "🇺🇸",
  city: "Los Angeles",
  ispTag: "Dual ISP 9929",
  host: "38.55.142.88",
  vlessPort: 443,
  panelUrl: "http://38.55.142.88:2053/demo",
  panelUsername: "admin",
  subUrl: "http://38.55.142.88:2096/sub/demo7a9f2c8e1b4d",
  mihomoSubUrl: "http://38.55.142.88:2096/mihomo/demo7a9f2c8e1b4d",
  sshPort: 22,
  sshUser: "root",
  realityTarget: "www.cisco.com",
  monthlyTotalGb: 2000,
  resetDay: 25,
  createdAt: new Date().toISOString(),
  source: "demo",
};

function buildBrowserPreviewData() {
  const now = Date.now();
  const history = Array.from({ length: 42 }, (_, idx) => {
    const p = idx * 0.42;
    const vpsDown = Math.round(5_800_000 + Math.sin(p * 0.75) * 3_100_000 + Math.cos(p * 1.9) * 1_100_000);
    const vpsUp = Math.round(980_000 + Math.cos(p * 0.65) * 420_000);
    return {
      ts: now - (41 - idx) * 2000,
      rttMs: Math.round(132 + Math.sin(p) * 6 + Math.cos(p * 1.8) * 2.5),
      vpsDownBps: Math.max(400_000, vpsDown),
      vpsUpBps: Math.max(90_000, vpsUp),
      localDownBps: Math.max(360_000, Math.round(vpsDown * 0.92)),
      localUpBps: Math.max(80_000, Math.round(vpsUp * 0.88)),
    };
  });

  const last = history[history.length - 1];
  const snapshot: TelemetrySnapshot = {
    nodeId: "demo-la",
    timestamp: now,
    healthScore: 99,
    healthStatus: "optimal",
    rttMs: last.rttMs,
    jitterMs: 2.8,
    packetLossPct: 0,
    xui: {
      connected: true,
      error: null,
      cpuPercent: 8.4,
      memUsed: 406 * 1024 * 1024,
      memTotal: 1024 * 1024 * 1024,
      swapUsed: 64 * 1024 * 1024,
      swapTotal: 1024 * 1024 * 1024,
      diskUsed: 3.4 * 1024 ** 3,
      diskTotal: 20 * 1024 ** 3,
      uptimeSecs: 18 * 86400 + 9 * 3600 + 24 * 60,
      loads: [0.12, 0.16, 0.14],
      xrayState: "running",
      xrayVersion: "25.3.6",
      netUpBps: last.vpsUpBps,
      netDownBps: last.vpsDownBps,
      netSentTotal: 198.4 * 1024 ** 3,
      netRecvTotal: 184.1 * 1024 ** 3,
      tcpCount: 44,
      udpCount: 4,
    },
    clients: [
      {
        email: "Shared",
        enable: true,
        upBytes: 35.8 * 1024 ** 3,
        downBytes: 186.2 * 1024 ** 3,
        totalBytes: 2000 * 1024 ** 3,
        resetDay: 25,
        online: true,
        subId: "demo7a9f2c8e1b4d",
        subUrl: "http://38.55.142.88:2096/sub/demo7a9f2c8e1b4d",
      },
    ],
    localShield: {
      mihomoConnected: true,
      mihomoSource: "unix-socket",
      mode: "rule",
      localUpBps: last.localUpBps,
      localDownBps: last.localDownBps,
      localUpTotal: 1_240_000_000,
      localDownTotal: 6_820_000_000,
      activeConnections: 38,
      proxyConnections: 24,
      directConnections: 14,
      ipv6Disabled: true,
      tunEnabled: true,
      tunStack: "mixed",
      fakeIpEnabled: true,
      quicBlocked: true,
      tcpConcurrent: true,
      exitIpv4: "38.55.142.88",
      ipv6Leak: false,
      matchesVpsIp: true,
    },
    history,
  };

  const probes: AiProbeResult[] = [
    { name: "Anthropic API", host: "api.anthropic.com", ok: true, statusCode: 404, latencyMs: 168, coldLatencyMs: 318 },
    { name: "OpenAI API", host: "api.openai.com", ok: true, statusCode: 401, latencyMs: 184, coldLatencyMs: 342 },
    { name: "Gemini API", host: "generativelanguage.googleapis.com", ok: true, statusCode: 404, latencyMs: 152, coldLatencyMs: 289 },
    { name: "GitHub API", host: "api.github.com", ok: true, statusCode: 200, latencyMs: 196, coldLatencyMs: 356 },
    { name: "Google 204", host: "www.google.com", ok: true, statusCode: 204, latencyMs: 141, coldLatencyMs: 274 },
  ];

  const history24h: HistoryBucket[] = Array.from({ length: 96 }, (_, idx) => {
    const rev = 95 - idx;
    const p = rev * 0.25;
    const peak = rev >= 12 && rev <= 28 ? 14 : 0;
    const avg = Math.round(133 + peak + Math.sin(p) * 4.5);
    return {
      ts: now - rev * 15 * 60_000,
      avgRttMs: avg,
      p95RttMs: Math.round(avg + 8 + Math.abs(Math.cos(p * 1.4)) * 6),
      lossPct: 0,
      avgLocalDownBps: Math.round(2_600_000 + Math.abs(Math.cos(p)) * 3_200_000),
      avgVpsDownBps: Math.round(2_900_000 + Math.abs(Math.cos(p)) * 3_500_000),
      samples: 30,
    };
  });

  const audit: AuditReport = {
    nodeId: "demo-la",
    checkedAt: new Date().toISOString(),
    score: 100,
    fail2banBannedTotal: 148,
    fail2banBannedCurrent: 2,
    items: [
      { key: "qdisc", label: "网卡队列 FQ (Fair Queue)", passed: true, actual: "fq", expected: "fq", severity: "critical" },
      { key: "bbr", label: "TCP 拥塞控制算法", passed: true, actual: "bbr", expected: "bbr", severity: "critical" },
      { key: "tfo", label: "TCP Fast Open (防 SYN 黑洞)", passed: true, actual: "0", expected: "0 (禁用)", severity: "warn" },
      { key: "ecn", label: "TCP ECN (防运营商丢包)", passed: true, actual: "0", expected: "0 (禁用)", severity: "warn" },
      { key: "keepalive", label: "TCP Keepalive 保活周期", passed: true, actual: "120s", expected: "120s", severity: "warn" },
      { key: "xui", label: "3x-ui / Xray 守护进程", passed: true, actual: "active", expected: "active", severity: "critical" },
      { key: "ufw", label: "UFW 最小暴露防火墙", passed: true, actual: "Status: active", expected: "Status: active", severity: "critical" },
      { key: "swap", label: "小内存 Swap 防 OOM 保护", passed: true, actual: "已启用 (1GB)", expected: ">= 1GB Swap", severity: "info" },
      { key: "fail2ban", label: "Fail2ban SSH 防爆破", passed: true, actual: "运行中（当前封禁 2 / 累计 148）", expected: "sshd jail active", severity: "warn" },
    ],
    error: null,
  };

  const carrierQuality: CarrierQualitySnapshot = {
    nodeId: "demo-la",
    updatedAt: now,
    error: null,
    carriers: [
      {
        key: "ct",
        label: "电信",
        target: "202.96.209.133",
        latestRttMs: 127,
        lossPct: 0.0,
        samples: Array.from({ length: 24 }, (_, i) => ({
          ts: now - (23 - i) * 60_000,
          rttMs: Math.round(126 + Math.sin(i * 0.5) * 4),
          lossPct: 0,
        })),
      },
      {
        key: "cu",
        label: "联通",
        target: "119.6.6.6",
        latestRttMs: 135,
        lossPct: 0.0,
        samples: Array.from({ length: 24 }, (_, i) => ({
          ts: now - (23 - i) * 60_000,
          rttMs: Math.round(134 + Math.cos(i * 0.45) * 3),
          lossPct: 0,
        })),
      },
      {
        key: "cm",
        label: "移动",
        target: "211.136.112.50",
        latestRttMs: 130,
        lossPct: 1.4,
        samples: Array.from({ length: 24 }, (_, i) => ({
          ts: now - (23 - i) * 60_000,
          rttMs: Math.round(130 + Math.sin(i * 0.6) * 5),
          lossPct: i === 1 ? 33.3 : 0,
        })),
      },
    ],
  };

  return { snapshot, probes, audit, history24h, carrierQuality };
}

export function useTelemetry() {
  const [nodes, setNodes] = useState<NodeConfig[]>([]);
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<TelemetrySnapshot | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [aiProbes, setAiProbes] = useState<AiProbeResult[]>([]);
  const [leakResult, setLeakResult] = useState<LeakCheckResult | null>(null);
  const [auditReport, setAuditReport] = useState<AuditReport | null>(null);
  const [carrierQuality, setCarrierQuality] = useState<CarrierQualitySnapshot | null>(null);
  const [history24h, setHistory24h] = useState<HistoryBucket[]>([]);
  const [isProbing, setIsProbing] = useState(false);
  const [isProbingCarriers, setIsProbingCarriers] = useState(false);
  const [isAuditing, setIsAuditing] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimerRef = useRef<number | null>(null);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    if (toastTimerRef.current !== null) {
      window.clearTimeout(toastTimerRef.current);
    }
    toastTimerRef.current = window.setTimeout(() => {
      setToast((prev) => (prev === msg ? null : prev));
      toastTimerRef.current = null;
    }, 3200);
  }, []);

  const hydrate = useCallback(async () => {
    if (!isTauriRuntime()) {
      const params = new URLSearchParams(window.location.search);
      const urlTheme = params.get("theme") === "obsidian" ? "obsidian" : "titanium";
      const preview = buildBrowserPreviewData();
      setNodes([DEMO_NODE]);
      setActiveNodeId(DEMO_NODE.id);
      setSnapshot(preview.snapshot);
      setAiProbes(preview.probes);
      setAuditReport(preview.audit);
      setCarrierQuality(preview.carrierQuality);
      setHistory24h(preview.history24h);
      setLeakResult({
        exitIpv4: "38.55.142.88",
        ipv6Leak: false,
        matchesVpsIp: true,
        checkedAt: new Date().toISOString(),
      });
      setSettings({
        trayDisplay: "speed-rtt",
        pollActiveSecs: 2,
        pollIdleSecs: 15,
        demoMode: false,
        autoCheckUpdates: true,
        inboxDir: "~/.local/share/native-isp-vps/inbox",
        theme: urlTheme,
      });
      return;
    }
    try {
      const [n, active, snap, cfg, diag] = await Promise.all([
        api.listNodes(),
        api.getActiveNodeId(),
        api.getLatestTelemetry(),
        api.getSettings(),
        api.getCachedDiagnostics(),
      ]);
      setNodes(n);
      setActiveNodeId(active);
      setSettings(cfg);
      if (diag.aiProbes.length > 0) setAiProbes(diag.aiProbes);
      if (diag.leakResult) setLeakResult(diag.leakResult);
      if (diag.auditReport) setAuditReport(diag.auditReport);
      if (diag.carrierQuality) setCarrierQuality(diag.carrierQuality);
      if (snap) {
        setSnapshot(snap);
      } else {
        const fresh = await api.triggerRefresh();
        if (fresh) setSnapshot(fresh);
      }
    } catch {
      showToast("初始化遥测状态失败，请稍后重试");
    }
  }, [showToast]);

  useEffect(() => {
    hydrate();

    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const register = (p: Promise<() => void>) => {
      p.then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      }).catch(() => {});
    };

    register(
      subscribeEvent<TelemetrySnapshot>("cockpit://telemetry", (snap) => {
        setSnapshot(snap);
      })
    );
    register(
      subscribeEvent<NodeConfig[]>("cockpit://nodes-updated", (updated) => {
        setNodes(updated);
      })
    );
    register(
      subscribeEvent<string | null>("cockpit://active-node-updated", (nextId) => {
        setActiveNodeId(nextId);
      })
    );
    register(
      subscribeEvent<NodeConfig>("cockpit://node-imported", (node) => {
        setActiveNodeId(node.id);
        showToast(`已自动纳管新节点：${node.name}`);
      })
    );
    register(
      subscribeEvent<AppSettings>("cockpit://settings-updated", (cfg) => {
        setSettings(cfg);
      })
    );
    register(
      subscribeEvent<AiProbeResult[]>("cockpit://ai-probes-updated", (probes) => {
        setAiProbes(probes);
      })
    );
    register(
      subscribeEvent<LeakCheckResult>("cockpit://leak-updated", (leak) => {
        setLeakResult(leak);
      })
    );
    register(
      subscribeEvent<AuditReport>("cockpit://audit-updated", (report) => {
        setAuditReport(report);
      })
    );
    register(
      subscribeEvent<CarrierQualitySnapshot>("cockpit://carrier-quality-updated", (quality) => {
        setCarrierQuality(quality);
      })
    );

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
      if (toastTimerRef.current !== null) {
        window.clearTimeout(toastTimerRef.current);
        toastTimerRef.current = null;
      }
    };
  }, [hydrate, showToast]);

  const selectNode = useCallback(
    async (id: string) => {
      setActiveNodeId(id);
      if (isTauriRuntime()) {
        try {
          await api.setActiveNode(id);
        } catch {
          showToast("切换节点失败");
        }
      }
    },
    [showToast]
  );

  const addNode = useCallback(
    async (payload: HandoffPayload) => {
      const created = await api.upsertNode(payload);
      setActiveNodeId(created.id);
      showToast(`节点已安全保存至本地 0600 凭据库：${created.name}`);
      return created;
    },
    [showToast]
  );

  const removeNode = useCallback(
    async (id: string) => {
      try {
        const updated = await api.deleteNode(id);
        setNodes(updated);
        showToast("已移除节点并清理本地凭据");
      } catch {
        showToast("移除节点失败");
      }
    },
    [showToast]
  );

  const loadHistory24h = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const buckets = await api.getHistory24h(activeNodeId ?? undefined);
      setHistory24h(buckets);
    } catch {
      // Ignore transient history read error
    }
  }, [activeNodeId]);

  const runFullProbe = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setIsProbing(true);
    try {
      const [probes, leak] = await Promise.all([api.probeAiEndpoints(), api.runLeakCheck()]);
      setAiProbes(probes);
      setLeakResult(leak);
      await api.triggerRefresh();
      showToast("端到端 AI 连通性与防泄露验真完成");
    } catch {
      showToast("验真测速执行失败，请检查网络连接");
    } finally {
      setIsProbing(false);
    }
  }, [showToast]);

  const runCarrierProbe = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setIsProbingCarriers(true);
    try {
      const result = await api.probeCarrierQuality();
      if (result) setCarrierQuality(result);
      showToast(result?.error ? "三网探测遇到异常，请查看提示" : "三网回程延迟与丢包采样已更新");
    } catch {
      showToast("三网探测调用失败");
    } finally {
      setIsProbingCarriers(false);
    }
  }, [showToast]);

  const runAudit = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setIsAuditing(true);
    try {
      const report = await api.runSshAudit(activeNodeId ?? undefined);
      setAuditReport(report);
      showToast(report.error ? "SSH 体检未完成，请查看详情" : `VPS 内核深度体检得分 ${report.score}/100`);
    } catch {
      showToast("SSH 体检调用失败");
    } finally {
      setIsAuditing(false);
    }
  }, [activeNodeId, showToast]);

  const saveSettings = useCallback(
    async (patch: Partial<AppSettings>) => {
      if (!settings) return;
      const merged = { ...settings, ...patch };
      if (!isTauriRuntime()) {
        setSettings(merged);
        showToast("偏好设置已更新");
        return;
      }
      try {
        const saved = await api.updateSettings(merged);
        setSettings(saved);
        showToast("偏好设置已更新");
      } catch {
        showToast("保存偏好设置失败");
      }
    },
    [settings, showToast]
  );

  const isDemo = Boolean(settings?.demoMode || nodes.length === 0);
  const theme = settings?.theme === "obsidian" ? "obsidian" : "titanium";

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  const activeNode: NodeConfig = isDemo
    ? DEMO_NODE
    : nodes.find((n) => n.id === activeNodeId) ?? nodes[0] ?? DEMO_NODE;

  return {
    nodes,
    activeNode,
    activeNodeId,
    isDemo,
    theme,
    snapshot,
    settings,
    aiProbes,
    leakResult,
    auditReport,
    carrierQuality,
    history24h,
    isProbing,
    isProbingCarriers,
    isAuditing,
    toast,
    showToast,
    selectNode,
    addNode,
    removeNode,
    loadHistory24h,
    runFullProbe,
    runCarrierProbe,
    runAudit,
    saveSettings,
    refreshNow: async () => {
      if (isTauriRuntime()) {
        const s = await api.triggerRefresh();
        if (s) setSnapshot(s);
      }
    },
  };
}
