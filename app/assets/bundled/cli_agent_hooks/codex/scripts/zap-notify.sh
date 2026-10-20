#!/bin/bash
# Emit OSC 777 warp://cli-agent into the Zap/Warp PTY.
# Never write to stdout: Codex Stop/Interrupt treat non-JSON stdout as failure.
TITLE="${1:-warp://cli-agent}"
BODY="${2:-}"
SEQ=$(printf '\033]777;notify;%s;%s\007' "$TITLE" "$BODY")
emitted=0
if [ -w /dev/tty ]; then
  if printf '%s' "$SEQ" > /dev/tty 2>/dev/null; then emitted=1; fi
fi
if [ "$emitted" -eq 0 ]; then
  TTY_DEV=$(tty 2>/dev/null || true)
  if [ -n "$TTY_DEV" ] && [ -w "$TTY_DEV" ]; then
    if printf '%s' "$SEQ" > "$TTY_DEV" 2>/dev/null; then emitted=1; fi
  fi
fi
# Last resort: write to stderr's tty if available (still not stdout).
if [ "$emitted" -eq 0 ] && [ -w /dev/stderr ]; then
  printf '%s' "$SEQ" > /dev/stderr 2>/dev/null || true
fi
exit 0
