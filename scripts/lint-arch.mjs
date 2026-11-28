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
    if (/\?\?\s*["']\d+\.\d+\.\d+["']/.test(line)) {
      failures.push(
        `[NO_HARDCODED_APP_VERSION] ${r}:${idx + 1}: hardcoded fallback app version literal (${line.trim()}).\n` +
          `  Why: Hardcoded version fallbacks drift silently across releases.\n` +
          `  Fix: Import 'pkg' from '../../package.json' and use 'pkg.version' as the single source of truth.`
      );
    }
  });
}

// 3. Rust file size, async hot-path hygiene & targeted telemetry emission
const rsFiles = walk(path.join(ROOT, "app/src-tauri/src"), [".rs"]);
const hotAsyncFiles = new Set([
  "app/src-tauri/src/carrier_probe.rs",
  "app/src-tauri/src/local_probe.rs",
  "app/src-tauri/src/mihomo.rs",
  "app/src-tauri/src/panel_tunnel.rs",
  "app/src-tauri/src/poller.rs",
  "app/src-tauri/src/inbox.rs",
]);
const SSH_BUILDER = "app/src-tauri/src/ssh.rs";

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

    if (r !== SSH_BUILDER && /Command::new\(\s*"ssh"\s*\)/.test(line)) {
      failures.push(
        `[SSH_BUILDER_ONLY] ${r}:${idx + 1}: raw 'Command::new("ssh")' outside ${SSH_BUILDER}.\n` +
          `  Why: Node fields come from AI handoff JSON; only the central builder validates host/user and inserts '--' so values like '-oProxyCommand=...' can never become ssh options.\n` +
          `  Fix: Use 'ssh::command(node, SshMode::..., &extra_args, remote_cmd)' from ${SSH_BUILDER}.`
      );
    }

    if (/\.emit\(\s*"cockpit:\/\/(telemetry|carrier-quality-updated)"/.test(line)) {
      failures.push(
        `[NO_GLOBAL_TELEMETRY_BROADCAST] ${r}:${idx + 1}: global telemetry emit detected (${trimmed}).\n` +
          `  Why: Broadcasting periodic telemetry wakes hidden WebViews (main/popover) and wastes CPU/battery.\n` +
          `  Fix: Call 'state::emit_to_visible_windows(app, ...)' in app/src-tauri/src/state.rs.`
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

  const invokedCmds = new Set([...tsText.matchAll(/invoke\("([a-z0-9_]+)"/g)].map((m) => m[1]));
  for (const cmd of invokedCmds) {
    if (!registeredCmds.has(cmd)) {
      failures.push(
        `[IPC_CONTRACT_PARITY] app/src/lib/tauri.ts invokes '${cmd}' which is missing from tauri::generate_handler![] in app/src-tauri/src/lib.rs.\n` +
          `  Fix: Register 'commands::${cmd}' in app/src-tauri/src/lib.rs or fix the command name in app/src/lib/tauri.ts.`
      );
    }
  }
  for (const cmd of registeredCmds) {
    if (!invokedCmds.has(cmd)) {
      failures.push(
        `[IPC_DEAD_COMMAND] app/src-tauri/src/lib.rs registers 'commands::${cmd}' but app/src/lib/tauri.ts never invokes it.\n` +
          `  Why: Unused IPC commands widen the WebView attack surface and drift silently.\n` +
          `  Fix: Add a typed method to 'api' in app/src/lib/tauri.ts, or remove the command from generate_handler![].`
      );
    }
  }
}

// 4b. Event contract parity: every event the frontend subscribes to must be emitted by Rust
const rustSources = rsFiles.map((f) => fs.readFileSync(f, "utf8")).join("\n");
for (const file of feFiles) {
  const text = fs.readFileSync(file, "utf8");
  for (const m of text.matchAll(/subscribeEvent<[^>]*>\(\s*"(cockpit:\/\/[a-z0-9-]+)"/g)) {
    if (!rustSources.includes(`"${m[1]}"`)) {
      failures.push(
        `[EVENT_CONTRACT_PARITY] ${rel(file)} subscribes to '${m[1]}' but no Rust source under app/src-tauri/src emits it.\n` +
          `  Fix: Emit it via app.emit / state::emit_to_visible_windows, or fix the event name.`
      );
    }
  }
}

// 4c. WebView hardening: a strict CSP must stay configured
const tauriConf = JSON.parse(fs.readFileSync(path.join(ROOT, "app/src-tauri/tauri.conf.json"), "utf8"));
const csp = tauriConf?.app?.security?.csp;
const scriptSrc = typeof csp === "string" ? csp : csp?.["script-src"] ?? csp?.["default-src"] ?? "";
if (!csp || /unsafe-inline|unsafe-eval/.test(scriptSrc)) {
  failures.push(
    `[CSP_REQUIRED] app/src-tauri/tauri.conf.json: app.security.csp must be set and script-src must not allow 'unsafe-inline'/'unsafe-eval' (got ${JSON.stringify(csp)}).\n` +
      `  Why: Without CSP any injected markup (e.g. a crafted node name) could run script with IPC access.\n` +
      `  Fix: Restore the object CSP with script-src 'self' and connect-src 'ipc: http://ipc.localhost'.`
  );
}

// 4d. Network doc invariant: never recommend `rotate` for VPS resolvers (one dead upstream ⇒ ~1/3 lookups stall)
for (const doc of ["SKILL.md", ...walk(path.join(ROOT, "references"), [".md", ".conf", ".yaml"]).map(rel)]) {
  const full = path.join(ROOT, doc);
  if (!fs.existsSync(full)) continue;
  fs.readFileSync(full, "utf8")
    .split("\n")
    .forEach((line, idx) => {
      if (/^\s*options\b.*\brotate\b/.test(line)) {
        failures.push(
          `[NO_RESOLV_ROTATE] ${doc}:${idx + 1}: resolv.conf template uses 'rotate' (${line.trim()}).\n` +
            `  Why: REALITY resolves its camouflage target per connection; with rotate, one unreachable upstream stalls ~1/3 of handshakes by the resolver timeout.\n` +
            `  Fix: Probe upstreams first, keep only reachable ones in order, and use 'options timeout:1 attempts:2'.`
        );
      }
    });
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

// 6. Release version parity across package.json, Cargo.toml, and tauri.conf.json
const pkgVer = JSON.parse(fs.readFileSync(path.join(ROOT, "app/package.json"), "utf8")).version;
const tauriVer = JSON.parse(
  fs.readFileSync(path.join(ROOT, "app/src-tauri/tauri.conf.json"), "utf8")
).version;
const cargoToml = fs.readFileSync(path.join(ROOT, "app/src-tauri/Cargo.toml"), "utf8");
const cargoVer = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!(pkgVer && tauriVer && cargoVer && pkgVer === tauriVer && tauriVer === cargoVer)) {
  failures.push(
    `[VERSION_PARITY] Version mismatch across manifests: package.json=${pkgVer}, Cargo.toml=${cargoVer}, tauri.conf.json=${tauriVer}.\n` +
      `  Why: Release tags & Tauri updater rely on identical version numbers across JS/Rust/Tauri configs.\n` +
      `  Fix: Align 'version' in app/package.json, app/src-tauri/Cargo.toml, and app/src-tauri/tauri.conf.json.`
  );
}

// 6b. Release notes parity: the release workflow publishes CHANGELOG.md's section for the tag
const changelogPath = path.join(ROOT, "CHANGELOG.md");
const changelog = fs.existsSync(changelogPath) ? fs.readFileSync(changelogPath, "utf8") : "";
if (!new RegExp(`^## v${pkgVer.replace(/\./g, "\\.")}\\b`, "m").test(changelog)) {
  failures.push(
    `[CHANGELOG_PARITY] CHANGELOG.md has no '## v${pkgVer}' section.\n` +
      `  Why: .github/workflows/release-app.yml uses that section as the GitHub Release body.\n` +
      `  Fix: Add a '## v${pkgVer} — <date>' section at the top of CHANGELOG.md describing the release.`
  );
}

// 7. Prevent accidental tracking of private credential files or real Bearer tokens in repo docs
const forbiddenSecretFiles = walk(ROOT, [".key", ".pem", "secrets.json", "nodes.json"]);
for (const file of forbiddenSecretFiles) {
  failures.push(
    `[NO_TRACKED_SECRETS] ${rel(file)}: sensitive credential or private key file found in repository.\n` +
      `  Why: SSH keys, signing keys, and node credentials must never enter the Git workspace.\n` +
      `  Fix: Remove the file from the repository and store it under ~/.config/native-isp-vps/ or ~/.ssh/.`
  );
}

if (failures.length > 0) {
  console.error(`lint-arch failed with ${failures.length} violation(s):\n\n${failures.join("\n\n")}`);
  process.exit(1);
}

console.log(
  `lint-arch PASS (${feFiles.length} TS/TSX files, ${rsFiles.length} Rust files, IPC/version parity v${pkgVer} & Mihomo rules verified, 0 violations)`
);
