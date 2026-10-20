#!/bin/bash
# Build warp://cli-agent JSON for Grok hooks. agent is always "grok".
PLUGIN_CURRENT_PROTOCOL_VERSION=1
negotiate_protocol_version() {
  local warp_version="${WARP_CLI_AGENT_PROTOCOL_VERSION:-1}"
  if [ "$warp_version" -lt "$PLUGIN_CURRENT_PROTOCOL_VERSION" ] 2>/dev/null; then
    echo "$warp_version"
  else
    echo "$PLUGIN_CURRENT_PROTOCOL_VERSION"
  fi
}
build_payload() {
  local input="$1"
  local event="$2"
  shift 2
  local protocol_version
  protocol_version=$(negotiate_protocol_version)
  local session_id cwd project
  session_id=$(echo "$input" | jq -r '.session_id // .sessionId // empty' 2>/dev/null)
  cwd=$(echo "$input" | jq -r '.cwd // empty' 2>/dev/null)
  project=""
  if [ -n "$cwd" ]; then
    project=$(basename "$cwd")
  fi
  jq -nc \
    --argjson v "$protocol_version" \
    --arg agent "grok" \
    --arg event "$event" \
    --arg session_id "$session_id" \
    --arg cwd "$cwd" \
    --arg project "$project" \
    "$@" \
    '{v:$v, agent:$agent, event:$event, session_id:$session_id, cwd:$cwd, project:$project} + $ARGS.named'
}
