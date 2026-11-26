#!/usr/bin/env node
/**
 * AI-Native Garden Architecture & Static Guardrail Gate (zero external dependencies).
 * Enforces L1/L2 invariants with Error-Driven messages so AI agents fix violations on the first try.
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

// 3. Rust file size check
const rsFiles = walk(path.join(ROOT, "app/src-tauri/src"), [".rs"]);
for (const file of rsFiles) {
  const lines = fs.readFileSync(file, "utf8").split("\n");
  if (lines.length > MAX_LINES) {
    failures.push(
      `[FILE_SIZE] ${rel(file)}: ${lines.length} lines exceeds max ${MAX_LINES}.\n` +
        `  Fix: Extract cohesive responsibilities into a dedicated module under app/src-tauri/src/.`
    );
  }
}

if (failures.length > 0) {
  console.error(`lint-arch failed with ${failures.length} violation(s):\n\n${failures.join("\n\n")}`);
  process.exit(1);
}

console.log(
  `lint-arch PASS (${feFiles.length} TS/TSX files, ${rsFiles.length} Rust files, 0 violations)`
);
