#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/build-payload.sh"
INPUT=$(cat)
BODY=$(build_payload "$INPUT" "session_start" --arg plugin_version "1.0.0")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
