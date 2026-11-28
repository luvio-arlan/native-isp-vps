import type { ClientQuota, NodeConfig } from "../types/telemetry";

const GIB = 1024 ** 3;

export interface QuotaSummary {
  usedBytes: number;
  totalBytes: number;
  /** Label for the quota owner: the single shared client's email, or null for multi-member nodes. */
  ownerLabel: string | null;
}

/**
 * Single source of truth for the node-level monthly quota shown in Overview, HUD and Users:
 * a single shared 3x-ui client with its own `total` wins; otherwise sum members' explicit
 * totals, falling back to the node's configured monthly allowance.
 */
export function summarizeQuota(node: NodeConfig, clients: ClientQuota[]): QuotaSummary {
  const usedBytes = clients.reduce((acc, c) => acc + c.upBytes + c.downBytes, 0);
  const nodeTotal = node.monthlyTotalGb * GIB;
  if (clients.length === 1) {
    const only = clients[0];
    return {
      usedBytes,
      totalBytes: only.totalBytes > 0 ? only.totalBytes : nodeTotal,
      ownerLabel: only.email,
    };
  }
  return { usedBytes, totalBytes: nodeTotal, ownerLabel: null };
}

/** Per-member allowance: the client's own 3x-ui total, else the node's monthly allowance. */
export function clientTotalBytes(node: NodeConfig, client: ClientQuota): number {
  return client.totalBytes > 0 ? client.totalBytes : node.monthlyTotalGb * GIB;
}
