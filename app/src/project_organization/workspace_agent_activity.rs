use std::time::Duration;

use instant::Instant;

use crate::ai::agent::conversation::ConversationStatus;
use crate::terminal::cli_agent_sessions::CLIAgentSessionStatus;
use crate::terminal::CLIAgent;

/// Done residual keeps the green check this long, then presentation demotes to Idle (spec §5.4).
pub(crate) const DONE_RESIDUAL_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Compute-on-read Done→Idle demotion from `phase_changed_at` age.
pub(crate) fn demote_done_phase(
    phase: WorkspaceAgentPhase,
    phase_changed_at: Option<Instant>,
    now: Instant,
) -> WorkspaceAgentPhase {
    if phase != WorkspaceAgentPhase::Done {
        return phase;
    }
    let Some(changed_at) = phase_changed_at else {
        // Without a timestamp we cannot age out; keep Done residual.
        return WorkspaceAgentPhase::Done;
    };
    if now.saturating_duration_since(changed_at) >= DONE_RESIDUAL_TTL {
        WorkspaceAgentPhase::Idle
    } else {
        WorkspaceAgentPhase::Done
    }
}

/// Delay until [`demote_done_phase`] would flip Done→Idle, if still inside the residual window.
pub(crate) fn duration_until_done_demotion(
    phase: WorkspaceAgentPhase,
    phase_changed_at: Option<Instant>,
    now: Instant,
) -> Option<Duration> {
    if phase != WorkspaceAgentPhase::Done {
        return None;
    }
    let changed_at = phase_changed_at?;
    let age = now.saturating_duration_since(changed_at);
    DONE_RESIDUAL_TTL.checked_sub(age).filter(|delay| !delay.is_zero())
}

/// workspace 行活动槽要展示的 agent 身份。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceAgentIdentity {
    Cli(CLIAgent),
    Oz { ambient: bool },
}

impl WorkspaceAgentIdentity {
    /// 树行空标题时的回退文案(对齐 Orca agent type label)。
    pub(crate) fn tree_label(self) -> &'static str {
        match self {
            Self::Cli(agent) => agent.tree_label(),
            Self::Oz { .. } => "Oz",
        }
    }
}

/// 页签行状态点语义(对齐 Orca AgentStateDot)。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceAgentPhase {
    Idle,
    InProgress,
    Blocked,
    Waiting,
    Done,
    Failed,
}

impl WorkspaceAgentPhase {
    /// 折叠 workspace 绿点只统计进行中 / 阻塞,不计 Done 残留。
    pub(crate) fn counts_as_busy(self) -> bool {
        matches!(self, Self::InProgress | Self::Blocked | Self::Waiting)
    }

    /// Working 状态需要独立转圈动画(不再绕品牌图标画呼吸环)。
    pub(crate) fn needs_status_spinner(self) -> bool {
        matches!(self, Self::InProgress)
    }

    pub(crate) fn accessibility_label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::InProgress => "Working",
            Self::Blocked => "Blocked",
            Self::Waiting => "Waiting for input",
            Self::Done => "Done",
            Self::Failed => "Failed",
        }
    }
}

/// 某个 workspace 当前应展示的 agent 活动。
///
/// `model_label` 使该类型不再 `Copy`(对齐 Orca CompactAgentRow 的可选 model 芯片)。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceAgentActivity {
    pub identity: WorkspaceAgentIdentity,
    pub phase: WorkspaceAgentPhase,
    /// When the current phase was entered; drives Orca-style relative time on the row.
    pub phase_changed_at: Option<instant::Instant>,
    /// Raw model id/name (trimmed). Chip shows a shortened label; hover shows this fuller string.
    /// `None` hides the chip.
    pub model_label: Option<String>,
}

/// workspace 行左侧活动槽: 头像、绿点、空槽互斥。
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Normalize a raw model id / display name into a compact chip label.
/// Empty / whitespace-only input yields `None` (chip hidden).
/// Prefer storing the raw string on activity and calling this at display time so hover can show the fuller id.
pub(crate) fn model_chip_label(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Provider-prefixed ids (`anthropic/claude-opus-4`) → short tail.
    let short = trimmed.rsplit('/').next().unwrap_or(trimmed).trim();
    if short.is_empty() {
        None
    } else {
        Some(short.to_owned())
    }
}

impl WorkspaceAgentActivity {
    /// Display / aggregation phase after Done residual TTL (compute-on-read).
    pub(crate) fn presentation_phase(&self) -> WorkspaceAgentPhase {
        self.presentation_phase_at(Instant::now())
    }

    pub(crate) fn presentation_phase_at(&self, now: Instant) -> WorkspaceAgentPhase {
        demote_done_phase(self.phase, self.phase_changed_at, now)
    }

    /// Apply Done→Idle demotion onto `phase` when aged out; keeps `phase_changed_at` for relative time.
    pub(crate) fn apply_done_demotion(&mut self) {
        self.apply_done_demotion_at(Instant::now());
    }

    pub(crate) fn apply_done_demotion_at(&mut self, now: Instant) {
        self.phase = self.presentation_phase_at(now);
    }

    /// Remaining delay until Done residual should flip to Idle (for repaint scheduling).
    pub(crate) fn duration_until_done_demotion(&self) -> Option<Duration> {
        self.duration_until_done_demotion_at(Instant::now())
    }

    pub(crate) fn duration_until_done_demotion_at(&self, now: Instant) -> Option<Duration> {
        duration_until_done_demotion(self.phase, self.phase_changed_at, now)
    }

    pub(crate) fn counts_as_busy(&self) -> bool {
        // Use presentation phase so demoted Idle never lights the collapsed green dot.
        self.presentation_phase().counts_as_busy()
    }

    pub(crate) fn needs_status_spinner(&self) -> bool {
        self.presentation_phase().needs_status_spinner()
    }

    /// 兼容旧调用点:呼吸环已废弃,仅 InProgress 需要状态转圈。
    pub(crate) fn should_breathe(&self) -> bool {
        self.needs_status_spinner()
    }

    /// Orca compact relative time (`now` / `2m` / `1h` / `1d`), if phase timestamp is known.
    pub(crate) fn relative_time_label(&self) -> Option<String> {
        self.phase_changed_at
            .map(crate::util::time_format::format_short_time_ago_since)
    }

    /// Compact chip text (provider prefix stripped), if known.
    pub(crate) fn model_chip_label(&self) -> Option<String> {
        model_chip_label(self.model_label.as_deref())
    }

    /// Fuller model string for chip hover (Orca `title={model}`); same as raw when unshortened.
    pub(crate) fn model_chip_tooltip(&self) -> Option<&str> {
        // Only when a chip would render.
        let _ = model_chip_label(self.model_label.as_deref())?;
        self.model_label
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
    }
}

pub(crate) struct OzConversationSource {
    pub status: ConversationStatus,
    pub is_empty: bool,
    pub is_entirely_passive: bool,
    pub is_ambient: bool,
    pub phase_changed_at: Option<instant::Instant>,
    pub model_label: Option<String>,
}

/// CLI session fields forwarded into tree activity.
pub(crate) type CliActivitySource = (
    CLIAgent,
    CLIAgentSessionStatus,
    Option<instant::Instant>,
    Option<String>,
);

/// 同一 terminal 上的候选顺序: ambient → Oz → CLI。last_agent_activity 因此让 CLI 覆盖 Oz。
pub(crate) fn activities_from_terminal_sources(
    cli: Option<CliActivitySource>,
    oz: Option<OzConversationSource>,
    ambient_in_progress: bool,
) -> Vec<WorkspaceAgentActivity> {
    let mut activities = Vec::new();

    if ambient_in_progress {
        activities.push(WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Oz { ambient: true },
            phase: WorkspaceAgentPhase::InProgress,
            phase_changed_at: None,
            model_label: None,
        });
    }

    if let Some(oz) = oz {
        if !oz.is_empty && !oz.is_entirely_passive {
            if let Some(phase) = phase_from_conversation_status(&oz.status) {
                let mut activity = WorkspaceAgentActivity {
                    identity: WorkspaceAgentIdentity::Oz {
                        ambient: oz.is_ambient,
                    },
                    phase,
                    phase_changed_at: oz.phase_changed_at,
                    model_label: oz.model_label,
                };
                activity.apply_done_demotion();
                activities.push(activity);
            }
        }
    }

    if let Some((agent, status, phase_changed_at, model_label)) = cli {
        if let Some(phase) = phase_from_cli_status(&status) {
            let mut activity = WorkspaceAgentActivity {
                identity: WorkspaceAgentIdentity::Cli(agent),
                phase,
                phase_changed_at,
                model_label,
            };
            activity.apply_done_demotion();
            activities.push(activity);
        }
    }

    activities
}

fn phase_from_cli_status(status: &CLIAgentSessionStatus) -> Option<WorkspaceAgentPhase> {
    match status {
        CLIAgentSessionStatus::InProgress => Some(WorkspaceAgentPhase::InProgress),
        CLIAgentSessionStatus::Blocked { .. } => Some(WorkspaceAgentPhase::Blocked),
        // Done 残留:成功后仍保留品牌图标 + 完成勾。
        CLIAgentSessionStatus::Success => Some(WorkspaceAgentPhase::Done),
    }
}

fn phase_from_conversation_status(status: &ConversationStatus) -> Option<WorkspaceAgentPhase> {
    match status {
        ConversationStatus::InProgress => Some(WorkspaceAgentPhase::InProgress),
        ConversationStatus::Blocked { .. } => Some(WorkspaceAgentPhase::Blocked),
        ConversationStatus::Success => Some(WorkspaceAgentPhase::Done),
        ConversationStatus::Error => Some(WorkspaceAgentPhase::Failed),
        // 取消后仍保留身份,降为 Idle(灰点),不把 Cancelled 拼进标题。
        ConversationStatus::Cancelled => Some(WorkspaceAgentPhase::Idle),
    }
}

#[cfg(test)]
#[path = "workspace_agent_activity_tests.rs"]
mod tests;
