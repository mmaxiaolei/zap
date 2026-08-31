use warpui::EntityId;

use crate::project_organization::domain::RepositoryWorkspaceId;
use crate::project_organization::workspace_agent_activity::{
    workspace_activity_slot, WorkspaceActivitySlot, WorkspaceAgentActivity,
};

/// 树内页签节点的稳定身份,对应所属 PaneGroup。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProjectTreeTabId(pub EntityId);

/// 页签子节点左侧活动槽。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TabNodeActivity {
    Agent(WorkspaceAgentActivity),
    RunningDot,
    Idle,
}

/// 某个 workspace 下一行页签的展示数据。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectTreeTabNode {
    pub id: ProjectTreeTabId,
    pub title: String,
    pub activity: TabNodeActivity,
    pub is_active: bool,
}

/// 将单页签的 agent / 长任务判定压成树节点活动槽。
pub(crate) fn tab_node_activity(
    agent: Option<WorkspaceAgentActivity>,
    has_running_terminal: bool,
) -> TabNodeActivity {
    match workspace_activity_slot(agent, has_running_terminal) {
        WorkspaceActivitySlot::Agent(activity) => TabNodeActivity::Agent(activity),
        WorkspaceActivitySlot::RunningDot => TabNodeActivity::RunningDot,
        WorkspaceActivitySlot::Empty => TabNodeActivity::Idle,
    }
}

/// 折叠父节点只用通用绿点;展开后活动落在子节点上,父节点永不画 agent 头像。
pub(crate) fn workspace_parent_activity_slot(
    expanded: bool,
    any_child_busy: bool,
) -> WorkspaceActivitySlot {
    if expanded || !any_child_busy {
        WorkspaceActivitySlot::Empty
    } else {
        WorkspaceActivitySlot::RunningDot
    }
}

/// 当前活动页签只属于当前活动 workspace;后台 workspace 的上次活动页签不高亮。
pub(crate) fn tab_is_active(
    tab_workspace_id: RepositoryWorkspaceId,
    tab_index: usize,
    active_workspace_id: Option<RepositoryWorkspaceId>,
    active_tab_index: usize,
) -> bool {
    active_workspace_id == Some(tab_workspace_id) && tab_index == active_tab_index
}

impl TabNodeActivity {
    pub(crate) fn is_busy(self) -> bool {
        match self {
            Self::Agent(_) | Self::RunningDot => true,
            Self::Idle => false,
        }
    }

    pub(crate) fn agent(self) -> Option<WorkspaceAgentActivity> {
        match self {
            Self::Agent(activity) => Some(activity),
            Self::RunningDot | Self::Idle => None,
        }
    }

    pub(crate) fn should_breathe(self) -> bool {
        self.agent()
            .is_some_and(WorkspaceAgentActivity::should_breathe)
    }
}

#[cfg(test)]
#[path = "project_tree_tab_tests.rs"]
mod tests;
