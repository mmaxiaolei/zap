#!/bin/bash
# Grok [[ui.notifications.hooks]] entrypoint (env: GROK_EVENT / GROK_MESSAGE / GROK_SESSION_ID).
# Always fires (only_unfocused=false) so focused Zap panes still clear the spinner.
# A finished Q&A must map to stop (Success/Done), never idle_prompt (Idle grey dot).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/build-payload.sh"
EVENT_RAW="${GROK_EVENT:-turn_complete}"
MSG="${GROK_MESSAGE:-}"
SID="${GROK_SESSION_ID:-}"
case "$EVENT_RAW" in
  turn_complete|task_complete) EVENT="stop" ;;
  approval_required) EVENT="permission_request" ;;
  agent_error) EVENT="error" ;;
  # session_ready after a turn still carries the completion message — treat as Done.
  session_ready)
    if [ -n "$MSG" ]; then EVENT="stop"; else EVENT="idle_prompt"; fi
    ;;
  *) EVENT="stop" ;;
esac
INPUT=$(jq -nc --arg sid "$SID" --arg msg "$MSG" '{session_id:$sid, summary:$msg, message:$msg, query:$msg}')
BODY=$(build_payload "$INPUT" "$EVENT" --arg summary "$MSG" --arg query "$MSG")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
