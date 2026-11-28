import type { AuditReport, NodeConfig, PanelTransport, TelemetrySnapshot } from "../types/telemetry";
import { formatBytes } from "./format";

export interface PanelTransportInfo {
  label: string;
  secure: boolean;
  hint: string;
}

/** Human-readable panel API transport (`null` before the first real poll / in demo mode). */
export function describePanelTransport(
  transport: PanelTransport | "" | undefined
): PanelTransportInfo | null {
  switch (transport) {
    case "ssh-tunnel":
      return { label: "SSH 加密隧道", secure: true, hint: "Token / 密码经 SSH 加密传输，从不明文出网" };
    case "tls":
      return { label: "HTTPS", secure: true, hint: "面板 API 经 TLS 加密" };
    case "paused":
      return {
        label: "隧道重连中",
        secure: true,
        hint: "SSH 隧道暂时中断：为防 Token 明文泄露已暂停面板 API，恢复后自动继续",
      };
    case "plaintext":
      return {
        label: "明文 HTTP ⚠",
        secure: false,
        hint: "面板 Token 以明文跨公网传输：请按 Stage 5.5 配置 SSH 体检私钥以自动启用加密隧道",
      };
    default:
      return null;
  }
}

export function buildAiDeployPrompt(): string {
  return `请使用 native-isp-vps skill 帮我全自动部署并调优海外 VPS（纯 TCP VLESS+Reality+Vision + BBR/FQ 内核加固 + 本机 Clash Verge Rev 优化），完成后按 Stage 5.5 将节点信息自动写入本地看板收件箱（~/.local/share/native-isp-vps/inbox/）实现免密自动接入。`;
}

export function buildAiRepairPrompt(
  node: NodeConfig | null,
  snap: TelemetrySnapshot | null,
  audit: AuditReport | null
): string {
  const lines: string[] = [
    "请使用 `native-isp-vps` skill 帮我排查并修复以下本地看板检测到的异常（以下诊断信息已自动脱敏）：",
    "",
    `- 节点名称：${node ? node.name : "当前节点"}`,
    `- 节点目标端口：${node?.vlessPort ?? 443}/tcp（Reality 目标：${node?.realityTarget ?? "未记录"}）`,
  ];

  if (snap) {
    lines.push(
      `- 实时链路：健康分 ${snap.healthScore}/100 · RTT ${snap.rttMs != null ? `${snap.rttMs}ms` : "超时"} · 抖动 ±${snap.jitterMs}ms · 丢包 ${snap.packetLossPct}%`
    );
    lines.push(
      `- VPS 资源：CPU ${snap.xui.cpuPercent.toFixed(1)}% · RAM ${formatBytes(snap.xui.memUsed)}/${formatBytes(snap.xui.memTotal)} · Swap ${formatBytes(snap.xui.swapUsed)}/${formatBytes(snap.xui.swapTotal)} · Xray ${snap.xui.xrayState}`
    );
    const sh = snap.localShield;
    lines.push(
      `- 本机 Clash Verge Rev：${sh.mihomoConnected ? `已连接 (${sh.mihomoSource})` : "未连接"} · TUN=${sh.tunEnabled ? sh.tunStack ?? "on" : "off"} · IPv6禁用=${String(sh.ipv6Disabled)} · QUIC拦截=${String(sh.quicBlocked)} · IPv6泄露=${String(sh.ipv6Leak)}`
    );
    if (snap.xui.panelTransport === "plaintext") {
      lines.push(
        "- [安全] 看板访问 3x-ui 面板 API 为明文 HTTP（Token 可被链路窃听）：请按 Stage 5.5 写入 SSH 体检公钥，看板将自动切换为 SSH 加密隧道"
      );
    }
  }

  if (audit) {
    const failed = audit.items.filter((i) => !i.passed);
    if (failed.length > 0) {
      lines.push("", "### SSH 深度体检未通过项：");
      for (const item of failed) {
        lines.push(`- [未通过] ${item.label}：当前为 \`${item.actual || "空"}\`，期望 \`${item.expected}\``);
      }
    } else if (audit.error) {
      lines.push("", `### SSH 体检提示：${audit.error}`);
    }
  }

  lines.push(
    "",
    "请按 `native-isp-vps` 的 Stage 1 / Stage 3 / Stage 5 标准逐项核验并自动修复，修复后重新端到端验真。"
  );

  return lines.join("\n");
}
