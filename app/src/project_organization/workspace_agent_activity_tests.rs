use crate::terminal::CLIAgent;

use super::{
    WorkspaceActivitySlot, WorkspaceAgentActivity, WorkspaceAgentIdentity, WorkspaceAgentPhase,
    last_agent_activity, workspace_activity_slot,
};

fn grok_running() -> WorkspaceAgentActivity {
    WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
        phase: WorkspaceAgentPhase::InProgress,
    }
}

fn claude_blocked() -> WorkspaceAgentActivity {
    WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Claude),
        phase: WorkspaceAgentPhase::Blocked,
    }
}

#[test]
fn last_agent_activity_returns_later_candidate() {
    assert_eq!(
        last_agent_activity([grok_running(), claude_blocked()]),
        Some(claude_blocked())
    );
}

#[test]
fn last_agent_activity_returns_none_when_empty() {
    assert_eq!(last_agent_activity([]), None);
}

#[test]
fn activity_slot_prefers_agent_over_running_dot() {
    assert_eq!(
        workspace_activity_slot(Some(grok_running()), true),
        WorkspaceActivitySlot::Agent(grok_running())
    );
}

#[test]
fn activity_slot_falls_back_to_running_dot() {
    assert_eq!(
        workspace_activity_slot(None, true),
        WorkspaceActivitySlot::RunningDot
    );
}

#[test]
fn activity_slot_is_empty_when_idle() {
    assert_eq!(
        workspace_activity_slot(None, false),
        WorkspaceActivitySlot::Empty
    );
}

#[test]
fn in_progress_activity_should_breathe() {
    assert!(grok_running().should_breathe());
}

#[test]
fn blocked_activity_should_not_breathe() {
    assert!(!claude_blocked().should_breathe());
}
