#!/usr/bin/env node
/**
 * AI-Native Garden Architecture & Static Guardrail Gate (zero external dependencies).
 * Enforces L1/L2/L3 invariants with Error-Driven messages so AI agents fix violations on the first try.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const MAX_LINES = 800;
const failures = [];

function walk(dir, exts) {
  const out = [];
  if (!fs.existsSync(dir)) return out;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (["node_modules", "dist", "target", ".git"].includes(entry.name)) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(full, exts));
    else if (exts.some((ext) => entry.name.endsWith(ext))) out.push(full);
  }
  return out;
}

function rel(file) {
  return path.relative(ROOT, file);
}

// 1. Hard ban on `keyring` crate in unsigned macOS desktop builds (enforces AGENTS.md #1)
for (const manifest of ["app/src-tauri/Cargo.toml", "app/src-tauri/Cargo.lock"]) {
  const full = path.join(ROOT, manifest);
  if (!fs.existsSync(full)) continue;
  const content = fs.readFileSync(full, "utf8");
  if (/^\s*name\s*=\s*"keyring"/m.test(content) || /^\s*keyring\s*=/m.test(content)) {
    failures.push(
      `[KEYCHAIN_BAN] ${manifest}: forbidden 'keyring' dependency detected.\n` +
        `  Why: Unsigned / ad-hoc macOS builds trigger infinite Keychain prompts on polling.\n` +
        `  Fix: Store node credentials in app/src-tauri/src/storage.rs (~/.config/native-isp-vps/secrets.json, 0600 + OnceLock cache).`
    );
  }
}

// 2. Frontend TypeScript & architecture checks
const feFiles = walk(path.join(ROOT, "app/src"), [".ts", ".tsx"]);
const escapeHatchRe = /\b(as\s+any|as\s+never|as\s+unknown\s+as)\b|@ts-ignore|@ts-expect-error/;
const rawInvokeRe = /from\s+["']@tauri-apps\/api\/core["']/;

for (const file of feFiles) {
  const text = fs.readFileSync(file, "utf8");
  const lines = text.split("\n");
  const r = rel(file);

  if (lines.length > MAX_LINES) {
    failures.push(
      `[FILE_SIZE] ${r}: ${lines.length} lines exceeds max ${MAX_LINES}.\n` +
        `  Fix: Split cohesive responsibilities into focused modules under app/src/.`
    );
  }

  lines.forEach((line, idx) => {
    if (escapeHatchRe.test(line)) {
      failures.push(
        `[TS_ESCAPE_HATCH] ${r}:${idx + 1}: forbidden TypeScript escape hatch (${line.trim()}).\n` +
          `  Fix: Use explicit types from app/src/types/telemetry.ts or proper type narrowing.`
      );
    }
    if (r !== "app/src/lib/tauri.ts" && rawInvokeRe.test(line)) {
      failures.push(
        `[IPC_BOUNDARY] ${r}:${idx + 1}: direct '@tauri-apps/api/core' import outside facade.\n` +
          `  Fix: Add a typed method to 'api' in app/src/lib/tauri.ts and call it from there.`
      );
    }
    if (/钥匙串|Keychain/i.test(line)) {
      failures.push(
        `[STALE_KEYCHAIN_COPY] ${r}:${idx + 1}: UI references OS Keychain.\n` +
          `  Fix: Describe actual local 0600 credential storage (~/.config/native-isp-vps/secrets.json).`
      );
    }
  });
}

// 3. Rust file size, async hot-path hygiene & targeted telemetry emission
const rsFiles = walk(path.join(ROOT, "app/src-tauri/src"), [".rs"]);
const hotAsyncFiles = new Set([
  "app/src-tauri/src/local_probe.rs",
  "app/src-tauri/src/mihomo.rs",
  "app/src-tauri/src/poller.rs",
  "app/src-tauri/src/inbox.rs",
]);

for (const file of rsFiles) {
  const text = fs.readFileSync(file, "utf8");
  const lines = text.split("\n");
  const r = rel(file);

  if (lines.length > MAX_LINES) {
    failures.push(
      `[FILE_SIZE] ${r}: ${lines.length} lines exceeds max ${MAX_LINES}.\n` +
        `  Fix: Extract cohesive responsibilities into a dedicated module under app/src-tauri/src/.`
    );
  }

  lines.forEach((line, idx) => {
    const trimmed = line.trim();
    if (trimmed.startsWith("//")) return;

    if (hotAsyncFiles.has(r) && /\bstd::fs::/.test(line)) {
      failures.push(
        `[NO_BLOCKING_FS_IN_HOT_ASYNC] ${r}:${idx + 1}: blocking 'std::fs' in hot async path (${trimmed}).\n` +
          `  Why: Synchronous disk I/O stalls Tokio worker threads during polling/watching.\n` +
          `  Fix: Use 'tokio::fs' or offload storage calls via 'tokio::task::spawn_blocking'.`
      );
    }

    if (/\.emit\(\s*"cockpit:\/\/telemetry"/.test(line)) {
      failures.push(
        `[NO_GLOBAL_TELEMETRY_BROADCAST] ${r}:${idx + 1}: global 'emit("cockpit://telemetry", ...)' detected.\n` +
          `  Why: Broadcasting high-frequency telemetry wakes hidden WebViews (main/popover) and wastes CPU/battery.\n` +
          `  Fix: Call 'state::emit_to_visible_windows(app, "cockpit://telemetry", &snapshot)' in app/src-tauri/src/state.rs.`
      );
    }
  });
}

// 4. Cross-end IPC contract parity check (app/src/lib/tauri.ts <-> app/src-tauri/src/lib.rs)
const tauriTsPath = path.join(ROOT, "app/src/lib/tauri.ts");
const rustLibPath = path.join(ROOT, "app/src-tauri/src/lib.rs");
if (fs.existsSync(tauriTsPath) && fs.existsSync(rustLibPath)) {
  const tsText = fs.readFileSync(tauriTsPath, "utf8");
  const rsText = fs.readFileSync(rustLibPath, "utf8");
  const handlerMatch = rsText.match(/generate_handler!\[([\s\S]*?)\]/);
  const registeredCmds = new Set(
    handlerMatch
      ? handlerMatch[1]
          .split(",")
          .map((s) => s.trim().replace(/^commands::/, ""))
          .filter(Boolean)
      : []
  );

  for (const m of tsText.matchAll(/invoke\("([a-z0-9_]+)"/g)) {
    const cmd = m[1];
    if (!registeredCmds.has(cmd)) {
      failures.push(
        `[IPC_CONTRACT_PARITY] app/src/lib/tauri.ts invokes '${cmd}' which is missing from tauri::generate_handler![] in app/src-tauri/src/lib.rs.\n` +
          `  Fix: Register 'commands::${cmd}' in app/src-tauri/src/lib.rs or fix the command name in app/src/lib/tauri.ts.`
      );
    }
  }
}

// 5. Proxy routing invariant: Fake-IP GEOIP rules must always carry `,no-resolve`
const mihomoRulesPath = path.join(ROOT, "references/mihomo-rules.yaml");
if (fs.existsSync(mihomoRulesPath)) {
  const ruleLines = fs.readFileSync(mihomoRulesPath, "utf8").split("\n");
  ruleLines.forEach((line, idx) => {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) return;
    if (/^GEOIP,/i.test(trimmed) && !/,no-resolve$/i.test(trimmed)) {
      failures.push(
        `[NO_GEOIP_WITHOUT_NO_RESOLVE] references/mihomo-rules.yaml:${idx + 1}: '${trimmed}' is missing ',no-resolve'.\n` +
          `  Why: GEOIP without 'no-resolve' forces synchronous local DoH lookups for every uncached overseas domain (+150-400ms latency), breaking Fake-IP instant dispatch.\n` +
          `  Fix: Append ',no-resolve' to the GEOIP rule.`
      );
    }
  });
}

if (failures.length > 0) {
  console.error(`lint-arch failed with ${failures.length} violation(s):\n\n${failures.join("\n\n")}`);
  process.exit(1);
}

console.log(
  `lint-arch PASS (${feFiles.length} TS/TSX files, ${rsFiles.length} Rust files, IPC parity & Mihomo rules verified, 0 violations)`
);
