#!/bin/bash
# Grok Notification → idle_prompt / permission_request / …
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/build-payload.sh"
INPUT=$(cat)
NOTIF_TYPE=$(echo "$INPUT" | jq -r '.notification_type // .notificationType // "unknown"' 2>/dev/null)
MSG=$(echo "$INPUT" | jq -r '.message // "Input needed"' 2>/dev/null)
[ -z "$MSG" ] && MSG="Input needed"
EVENT="$NOTIF_TYPE"
case "$NOTIF_TYPE" in
  idle_prompt) EVENT="idle_prompt" ;;
  permission_prompt) EVENT="permission_request" ;;
  task_complete) EVENT="stop" ;;
  *) EVENT="$NOTIF_TYPE" ;;
esac
BODY=$(build_payload "$INPUT" "$EVENT" --arg summary "$MSG")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
