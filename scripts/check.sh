#!/usr/bin/env bash
# Single-source-of-truth quality gate for native-isp-vps (used locally & in CI).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

echo "==> [1/4] Architecture & static guardrails (lint-arch)"
node scripts/lint-arch.mjs

echo "==> [2/4] Frontend typecheck & production build"
pnpm --dir app build

echo "==> [3/4] Rust clippy (zero warnings)"
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings

echo "==> [4/4] Rust unit tests"
cargo test --manifest-path app/src-tauri/Cargo.toml

echo "ALL CHECKS PASSED"
