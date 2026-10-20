#!/bin/bash
# Emit OSC 777 warp://cli-agent notification into the Zap/Warp PTY.
TITLE="${1:-warp://cli-agent}"
BODY="${2:-}"
case "${TERM_PROGRAM:-}" in
  WarpTerminal|ZapTerminal) ;;
  *)
    # Still try when nested / missing TERM_PROGRAM (hooks sometimes clear env).
    ;;
esac
SEQ=$(printf '\033]777;notify;%s;%s\007' "$TITLE" "$BODY")
emitted=0
# 1) controlling tty
if [ -w /dev/tty ]; then
  if printf '%s' "$SEQ" > /dev/tty 2>/dev/null; then emitted=1; fi
fi
# 2) `tty` device for this process
if [ "$emitted" -eq 0 ]; then
  TTY_DEV=$(tty 2>/dev/null || true)
  if [ -n "$TTY_DEV" ] && [ -w "$TTY_DEV" ]; then
    if printf '%s' "$SEQ" > "$TTY_DEV" 2>/dev/null; then emitted=1; fi
  fi
fi
# 3) stdout as last resort (some hosts forward it)
if [ "$emitted" -eq 0 ]; then
  printf '%s' "$SEQ"
fi
