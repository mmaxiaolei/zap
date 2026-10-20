#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/build-payload.sh"
INPUT=$(cat)
ERROR_MESSAGE=$(echo "$INPUT" | jq -r '.last_assistant_message // .lastAssistantMessage // .error // "Agent error"' 2>/dev/null)
BODY=$(build_payload "$INPUT" "error" --arg summary "$ERROR_MESSAGE")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
