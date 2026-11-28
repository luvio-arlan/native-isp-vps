#!/usr/bin/env bash
# Zero-Touch AI Handoff helper for Native ISP Cockpit
# Reads a handoff JSON payload from stdin and stages it atomically (0600) in the local Cockpit inbox.
# The running Cockpit App imports it into local 0600 credential storage and shreds the file within 1s.
# Malformed payloads are quarantined as *.json.invalid and reported via a system notification.

set -euo pipefail
umask 077

INBOX_DIR="${HOME}/.local/share/native-isp-vps/inbox"
mkdir -p "${INBOX_DIR}"
chmod 700 "${INBOX_DIR}"

STAMP="node-$(date +%s)-$$"
TMP_FILE="${INBOX_DIR}/.${STAMP}.partial"
OUT_FILE="${INBOX_DIR}/${STAMP}.json"
trap 'rm -f "${TMP_FILE}"' EXIT

cat > "${TMP_FILE}"
if command -v python3 >/dev/null 2>&1 && ! python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "${TMP_FILE}" 2>/dev/null; then
  echo "Handoff payload is not valid JSON; nothing was staged." >&2
  exit 1
fi
chmod 600 "${TMP_FILE}"
# Atomic rename: the app never observes a half-written manifest.
mv -f "${TMP_FILE}" "${OUT_FILE}"

echo "Handoff payload staged at ${OUT_FILE} (mode 600). Native ISP Cockpit will import secrets into local 0600 storage and shred the file automatically."
