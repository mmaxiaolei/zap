#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=build-payload.sh
source "$SCRIPT_DIR/build-payload.sh"
INPUT=$(cat)
QUERY=$(echo "$INPUT" | jq -r '.prompt // empty' 2>/dev/null)
if [ -n "$QUERY" ] && [ ${#QUERY} -gt 200 ]; then QUERY="${QUERY:0:197}..."; fi
BODY=$(build_payload "$INPUT" "prompt_submit" --arg query "$QUERY")
"$SCRIPT_DIR/zap-notify.sh" "warp://cli-agent" "$BODY"
# UserPromptSubmit: empty stdout (do not leak OSC / plain text into model context).
exit 0
