export interface NodeConfig {
  id: string;
  name: string;
  flag: string;
  city: string;
  ispTag: string;
  host: string;
  vlessPort: number;
  panelUrl: string;
  panelUsername?: string | null;
  subUrl?: string | null;
  mihomoSubUrl?: string | null;
  sshPort: number;
  sshUser: string;
  sshKeyPath?: string | null;
  realityTarget?: string | null;
  monthlyTotalGb: number;
  resetDay: number;
  createdAt: string;
  source: string;
}

export interface HandoffPayload {
  name?: string;
  flag?: string;
  city?: string;
  ispTag?: string;
  host: string;
  vlessPort?: number;
  panelUrl: string;
  panelUsername?: string;
  panelPassword?: string;
  apiToken?: string;
  subUrl?: string;
  mihomoSubUrl?: string;
  sshPort?: number;
  sshUser?: string;
  sshKeyPath?: string;
  realityTarget?: string;
  monthlyTotalGb?: number;
  resetDay?: number;
  source?: string;
}

export interface XuiStatus {
  connected: boolean;
  error?: string | null;
  cpuPercent: number;
  memUsed: number;
  memTotal: number;
  swapUsed: number;
  swapTotal: number;
  diskUsed: number;
  diskTotal: number;
  uptimeSecs: number;
  loads: [number, number, number];
  xrayState: string;
  xrayVersion: string;
  netUpBps: number;
  netDownBps: number;
  netSentTotal: number;
  netRecvTotal: number;
  tcpCount: number;
  udpCount: number;
}

export interface ClientQuota {
  email: string;
  enable: boolean;
  upBytes: number;
  downBytes: number;
  totalBytes: number;
  resetDay: number;
  online: boolean;
  subId?: string | null;
  subUrl?: string | null;
}

export interface TelemetryPoint {
  ts: number;
  rttMs?: number | null;
  vpsDownBps: number;
  vpsUpBps: number;
  localDownBps: number;
  localUpBps: number;
}

export interface AiProbeResult {
  name: string;
  host: string;
  ok: boolean;
  statusCode?: number | null;
  latencyMs?: number | null;
  error?: string | null;
}

export interface LocalShieldStatus {
  mihomoConnected: boolean;
  mihomoSource: string;
  mode: string;
  localUpBps: number;
  localDownBps: number;
  activeConnections: number;
  proxyConnections: number;
  directConnections: number;
  ipv6Disabled?: boolean | null;
  tunEnabled?: boolean | null;
  tunStack?: string | null;
  fakeIpEnabled?: boolean | null;
  quicBlocked?: boolean | null;
  tcpConcurrent?: boolean | null;
  exitIpv4?: string | null;
  ipv6Leak?: boolean | null;
  matchesVpsIp?: boolean | null;
}

export interface AuditItem {
  key: string;
  label: string;
  passed: boolean;
  actual: string;
  expected: string;
  severity: "critical" | "warn" | "info" | string;
}

export interface AuditReport {
  nodeId: string;
  checkedAt: string;
  score: number;
  fail2banBannedTotal?: number | null;
  fail2banBannedCurrent?: number | null;
  items: AuditItem[];
  error?: string | null;
}

export interface TelemetrySnapshot {
  nodeId: string;
  timestamp: number;
  healthScore: number;
  healthStatus: "optimal" | "degraded" | "critical" | "demo" | string;
  rttMs?: number | null;
  jitterMs: number;
  packetLossPct: number;
  xui: XuiStatus;
  clients: ClientQuota[];
  localShield: LocalShieldStatus;
  history: TelemetryPoint[];
}

export interface AppSettings {
  trayDisplay: "speed-rtt" | "rtt-only" | "dot-only" | string;
  pollActiveSecs: number;
  pollIdleSecs: number;
  demoMode: boolean;
  autoCheckUpdates: boolean;
  inboxDir: string;
  theme: "titanium" | "obsidian" | string;
}

export interface GitHubReleaseInfo {
  currentVersion: string;
  latestVersion?: string | null;
  hasUpdate: boolean;
  releaseUrl: string;
  releaseNotes?: string | null;
  publishedAt?: string | null;
  updaterReady: boolean;
}

export interface LeakCheckResult {
  exitIpv4?: string | null;
  ipv6Leak: boolean;
  matchesVpsIp: boolean;
  checkedAt: string;
}
