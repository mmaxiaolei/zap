#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/build-payload.sh"
INPUT=$(cat)
SUMMARY=$(echo "$INPUT" | jq -r '.tool_input.description // .tool_name // "Approval requested" ' 2>/dev/null)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null)
BODY=$(build_payload "$INPUT" "permission_request" \
  --arg summary "$SUMMARY" \
  --arg tool_name "$TOOL")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
exit 0
