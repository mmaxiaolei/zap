#!/bin/bash
# Codex Stop → warp://cli-agent stop (sidebar Done).
# Must exit 0 with no/JSON-only stdout — Codex rejects plain text on Stop.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=build-payload.sh
source "$SCRIPT_DIR/build-payload.sh"
INPUT=$(cat)
STOP_HOOK_ACTIVE=$(echo "$INPUT" | jq -r '.stop_hook_active // .stopHookActive // false' 2>/dev/null)
if [ "$STOP_HOOK_ACTIVE" = "true" ]; then
  exit 0
fi
QUERY=$(echo "$INPUT" | jq -r '.prompt // .last_user_message // empty' 2>/dev/null)
RESPONSE=$(echo "$INPUT" | jq -r '.last_assistant_message // .lastAssistantMessage // empty' 2>/dev/null)
if [ -n "$QUERY" ] && [ ${#QUERY} -gt 200 ]; then QUERY="${QUERY:0:197}..."; fi
if [ -n "$RESPONSE" ] && [ ${#RESPONSE} -gt 200 ]; then RESPONSE="${RESPONSE:0:197}..."; fi
BODY=$(build_payload "$INPUT" "stop" --arg query "$QUERY" --arg response "$RESPONSE")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
exit 0
