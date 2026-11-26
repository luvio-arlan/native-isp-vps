import type { AuditReport, NodeConfig, TelemetrySnapshot } from "../types/telemetry";
import { formatBytes } from "./format";

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
