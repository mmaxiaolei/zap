use crate::terminal::CLIAgent;

/// workspace 行活动槽要展示的 agent 身份。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceAgentIdentity {
    Cli(CLIAgent),
    Oz { ambient: bool },
}

/// 计入活动槽的会话阶段。结束 / 出错不进入此枚举。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceAgentPhase {
    InProgress,
    Blocked,
}

/// 某个 workspace 当前应展示的 agent 活动。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceAgentActivity {
    pub identity: WorkspaceAgentIdentity,
    pub phase: WorkspaceAgentPhase,
}

/// workspace 行左侧活动槽: 头像、绿点、空槽互斥。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceActivitySlot {
    Empty,
    RunningDot,
    Agent(WorkspaceAgentActivity),
}

/// 同一 workspace 内多个命中时取扫描顺序中最后一个。
pub(crate) fn last_agent_activity(
    activities: impl IntoIterator<Item = WorkspaceAgentActivity>,
) -> Option<WorkspaceAgentActivity> {
    activities.into_iter().last()
}

/// 有 agent 活动时只占头像槽; 否则回退绿点或空槽。
pub(crate) fn workspace_activity_slot(
    agent: Option<WorkspaceAgentActivity>,
    has_running_terminal: bool,
) -> WorkspaceActivitySlot {
    match agent {
        Some(activity) => WorkspaceActivitySlot::Agent(activity),
        None if has_running_terminal => WorkspaceActivitySlot::RunningDot,
        None => WorkspaceActivitySlot::Empty,
    }
}

impl WorkspaceAgentActivity {
    /// InProgress 需要呼吸环; Blocked 为静态环。
    pub(crate) fn should_breathe(self) -> bool {
        matches!(self.phase, WorkspaceAgentPhase::InProgress)
    }
}

#[cfg(test)]
#[path = "workspace_agent_activity_tests.rs"]
mod tests;
