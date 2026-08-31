use warpui::EntityId;

use crate::project_organization::domain::RepositoryWorkspaceId;
use crate::project_organization::workspace_agent_activity::{
    WorkspaceActivitySlot, WorkspaceAgentActivity, WorkspaceAgentIdentity, WorkspaceAgentPhase,
};
use crate::terminal::CLIAgent;

use super::{
    tab_is_active, tab_node_activity, workspace_parent_activity_slot, ProjectTreeTabId,
    ProjectTreeTabNode, TabNodeActivity,
};

fn grok_running() -> WorkspaceAgentActivity {
    WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
        phase: WorkspaceAgentPhase::InProgress,
    }
}

fn oz_blocked() -> WorkspaceAgentActivity {
    WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Oz { ambient: false },
        phase: WorkspaceAgentPhase::Blocked,
    }
}

#[test]
fn tab_node_activity_prefers_agent_over_running_dot() {
    assert_eq!(
        tab_node_activity(Some(grok_running()), true),
        TabNodeActivity::Agent(grok_running())
    );
}

#[test]
fn tab_node_activity_falls_back_to_running_dot() {
    assert_eq!(
        tab_node_activity(None, true),
        TabNodeActivity::RunningDot
    );
}

#[test]
fn tab_node_activity_is_idle_when_empty() {
    assert_eq!(tab_node_activity(None, false), TabNodeActivity::Idle);
}

#[test]
fn expanded_parent_never_shows_activity_slot() {
    assert_eq!(
        workspace_parent_activity_slot(true, true),
        WorkspaceActivitySlot::Empty
    );
    assert_eq!(
        workspace_parent_activity_slot(true, false),
        WorkspaceActivitySlot::Empty
    );
}

#[test]
fn collapsed_parent_shows_running_dot_when_any_child_is_busy() {
    assert_eq!(
        workspace_parent_activity_slot(false, true),
        WorkspaceActivitySlot::RunningDot
    );
}

#[test]
fn collapsed_idle_parent_is_empty() {
    assert_eq!(
        workspace_parent_activity_slot(false, false),
        WorkspaceActivitySlot::Empty
    );
}

#[test]
fn tab_is_active_only_for_current_workspace_active_index() {
    let workspace_a = RepositoryWorkspaceId(uuid::Uuid::from_u128(1));
    let workspace_b = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    assert!(tab_is_active(workspace_a, 1, Some(workspace_a), 1));
    assert!(!tab_is_active(workspace_a, 0, Some(workspace_a), 1));
    assert!(!tab_is_active(workspace_b, 1, Some(workspace_a), 1));
    assert!(!tab_is_active(workspace_a, 1, None, 1));
}

#[test]
fn in_progress_tab_activity_should_breathe() {
    assert!(TabNodeActivity::Agent(grok_running()).should_breathe());
    assert!(!TabNodeActivity::Agent(oz_blocked()).should_breathe());
    assert!(!TabNodeActivity::RunningDot.should_breathe());
    assert!(!TabNodeActivity::Idle.should_breathe());
}

#[test]
fn tab_node_busy_covers_agent_and_running_dot() {
    assert!(TabNodeActivity::Agent(grok_running()).is_busy());
    assert!(TabNodeActivity::RunningDot.is_busy());
    assert!(!TabNodeActivity::Idle.is_busy());
}

#[test]
fn project_tree_tab_id_is_stable_entity_id() {
    let id = ProjectTreeTabId(EntityId::from_usize(42));
    let node = ProjectTreeTabNode {
        id,
        title: "agent".to_string(),
        activity: TabNodeActivity::Idle,
        is_active: true,
    };
    assert_eq!(node.id, ProjectTreeTabId(EntityId::from_usize(42)));
}
