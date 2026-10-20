//! Infer CLI-agent mid-turn status from Zap-side signals when agent hooks
//! cannot deliver OSC 777 (e.g. Codex runs hooks with stderr suppressed /
//! without a PTY that reaches TerminalView).
//!
//! Codex TUI shows `Working (… • esc to interrupt)` while a turn runs; Zap
//! must not stay Idle in the sidebar until OSC Stop arrives.

use super::CLIAgent;
use super::CLIAgentSessionStatus;

/// Agents whose mid-turn status is unreliable from hooks alone when the user
/// types inside the agent TUI (not Zap rich-input).
pub(crate) fn needs_tui_mid_turn_inference(agent: CLIAgent) -> bool {
    matches!(
        agent,
        CLIAgent::Codex | CLIAgent::DeepSeek | CLIAgent::Grok
    )
}

/// True when PTY input looks like a prompt submit (Enter / CR).
/// Codex / Claude / Grok use Enter to submit; newline is usually a chord.
pub(crate) fn pty_bytes_look_like_submit(bytes: &[u8]) -> bool {
    bytes.contains(&b'\r')
}

/// Statuses where an Enter submit should move the sidebar to InProgress.
pub(crate) fn status_can_start_turn(status: &CLIAgentSessionStatus) -> bool {
    matches!(
        status,
        CLIAgentSessionStatus::Idle | CLIAgentSessionStatus::Success
    )
}

/// Codex (and similar) on-screen working indicator.
pub(crate) fn screen_indicates_agent_working(screen: &str) -> bool {
    // Codex TUI: "• Working (2s • esc to interrupt)"
    if screen.contains("esc to interrupt") {
        return true;
    }
    if screen.contains("Working (") {
        return true;
    }
    // Some builds use a leading bullet without ASCII "Working ("
    if screen.contains("Working…") || screen.contains("Working...") {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_cr_is_submit() {
        assert!(pty_bytes_look_like_submit(b"hello\r"));
        assert!(pty_bytes_look_like_submit(&[b'\r']));
        assert!(!pty_bytes_look_like_submit(b"hello"));
        assert!(!pty_bytes_look_like_submit(b"hello\n")); // naked LF alone — not Enter on macOS TUI
    }

    #[test]
    fn codex_working_line_detected() {
        let screen = "你是什么模型？\n\n• Working (2s • esc to interrupt)\n";
        assert!(screen_indicates_agent_working(screen));
    }

    #[test]
    fn idle_prompt_not_working() {
        let screen = "OpenAI Codex (v0.163.0)\nAsk Codex to do anything\n";
        assert!(!screen_indicates_agent_working(screen));
    }

    #[test]
    fn codex_needs_inference() {
        assert!(needs_tui_mid_turn_inference(CLIAgent::Codex));
        assert!(!needs_tui_mid_turn_inference(CLIAgent::Claude));
    }

    #[test]
    fn idle_and_success_can_start() {
        assert!(status_can_start_turn(&CLIAgentSessionStatus::Idle));
        assert!(status_can_start_turn(&CLIAgentSessionStatus::Success));
        assert!(!status_can_start_turn(&CLIAgentSessionStatus::InProgress));
        assert!(!status_can_start_turn(&CLIAgentSessionStatus::Waiting {
            message: None
        }));
    }
}
