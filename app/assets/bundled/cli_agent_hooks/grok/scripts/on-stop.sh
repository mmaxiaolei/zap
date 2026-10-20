#!/bin/bash
# Grok Stop → warp://cli-agent stop (clears sidebar spinner → Done)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=build-payload.sh
source "$SCRIPT_DIR/build-payload.sh"
INPUT=$(cat)
# Skip Stop gate continuation rounds (false idle). Prefer idle_prompt as backstop.
STOP_HOOK_ACTIVE=$(echo "$INPUT" | jq -r '.stop_hook_active // .stopHookActive // false' 2>/dev/null)
if [ "$STOP_HOOK_ACTIVE" = "true" ]; then
  exit 0
fi
# Nested subagent stops are not the session turn end.
SUBAGENT=$(echo "$INPUT" | jq -r '.subagentType // .subagent_type // empty' 2>/dev/null)
if [ -n "$SUBAGENT" ]; then
  exit 0
fi
QUERY=$(echo "$INPUT" | jq -r '.prompt // .last_user_message // empty' 2>/dev/null)
RESPONSE=$(echo "$INPUT" | jq -r '.last_assistant_message // .lastAssistantMessage // empty' 2>/dev/null)
if [ -n "$QUERY" ] && [ ${#QUERY} -gt 200 ]; then QUERY="${QUERY:0:197}..."; fi
if [ -n "$RESPONSE" ] && [ ${#RESPONSE} -gt 200 ]; then RESPONSE="${RESPONSE:0:197}..."; fi
BODY=$(build_payload "$INPUT" "stop" --arg query "$QUERY" --arg response "$RESPONSE")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
