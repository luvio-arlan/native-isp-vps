#!/usr/bin/env bash
# Zero-Touch AI Handoff helper for Native ISP Cockpit
# Reads a handoff JSON payload from stdin, writes it with 0600 permissions to the local Cockpit inbox.
# The running Cockpit App imports it into local 0600 credential storage and shreds this file within 1s.

set -euo pipefail

INBOX_DIR="${HOME}/.local/share/native-isp-vps/inbox"
mkdir -p "${INBOX_DIR}"
chmod 700 "${INBOX_DIR}"

OUT_FILE="${INBOX_DIR}/node-$(date +%s)-$$.json"
umask 077
cat > "${OUT_FILE}"
chmod 600 "${OUT_FILE}"

echo "Handoff payload staged at ${OUT_FILE} (mode 600). Native ISP Cockpit will import secrets into local 0600 storage and shred the file automatically."
