use crate::ai::agent::conversation::ConversationStatus;
use crate::terminal::cli_agent_sessions::CLIAgentSessionStatus;
use crate::terminal::CLIAgent;

use std::time::Duration;

use instant::Instant;

use super::{
    activities_from_terminal_sources, demote_done_phase, duration_until_done_demotion,
    last_agent_activity, model_chip_label, workspace_activity_slot, DONE_RESIDUAL_TTL,
    OzConversationSource, WorkspaceActivitySlot, WorkspaceAgentActivity, WorkspaceAgentIdentity,
    WorkspaceAgentPhase,
};

fn grok_running() -> WorkspaceAgentActivity {
    WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
        phase: WorkspaceAgentPhase::InProgress,
        phase_changed_at: None,
        model_label: None,
    }
}

fn claude_blocked() -> WorkspaceAgentActivity {
    WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Claude),
        phase: WorkspaceAgentPhase::Blocked,
        phase_changed_at: None,
        model_label: None,
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

#[test]
fn cli_in_progress_is_collected() {
    let activities = activities_from_terminal_sources(
        Some((CLIAgent::Grok, CLIAgentSessionStatus::InProgress, None, None)),
        None,
        false,
    );
    assert_eq!(activities, vec![grok_running()]);
}

#[test]
fn cli_success_keeps_done_residual() {
    let activities = activities_from_terminal_sources(
        Some((CLIAgent::Grok, CLIAgentSessionStatus::Success, None, None)),
        None,
        false,
    );
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
            phase: WorkspaceAgentPhase::Done,
            phase_changed_at: None,
            model_label: None,
        }]
    );
}

#[test]
fn done_residual_is_not_busy() {
    let done = WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
        phase: WorkspaceAgentPhase::Done,
        phase_changed_at: None,
        model_label: None,
    };
    assert!(!done.counts_as_busy());
    assert!(!done.needs_status_spinner());
}

#[test]
fn oz_user_gated_blocked_maps_to_waiting() {
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::Blocked {
                blocked_action: "ask".to_string(),
            },
            is_empty: false,
            is_entirely_passive: false,
            is_ambient: false,
            phase_changed_at: None,
            model_label: None,
        }),
        false,
    );
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Oz { ambient: false },
            phase: WorkspaceAgentPhase::Waiting,
            phase_changed_at: None,
            model_label: None,
        }]
    );
}

#[test]
fn cli_waiting_maps_to_waiting_phase() {
    let activities = activities_from_terminal_sources(
        Some((
            CLIAgent::Claude,
            CLIAgentSessionStatus::Waiting {
                message: Some("Approve Bash?".to_owned()),
            },
            None,
            None,
        )),
        None,
        false,
    );
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Cli(CLIAgent::Claude),
            phase: WorkspaceAgentPhase::Waiting,
            phase_changed_at: None,
            model_label: None,
        }]
    );
    assert!(activities[0].counts_as_busy());
    assert!(!activities[0].needs_status_spinner());
    assert_eq!(
        activities[0].presentation_phase().accessibility_label(),
        "Waiting for input"
    );
}

#[test]
fn cli_true_blocked_maps_to_blocked_phase() {
    let activities = activities_from_terminal_sources(
        Some((
            CLIAgent::Claude,
            CLIAgentSessionStatus::Blocked {
                message: Some("stuck".to_owned()),
            },
            None,
            None,
        )),
        None,
        false,
    );
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Cli(CLIAgent::Claude),
            phase: WorkspaceAgentPhase::Blocked,
            phase_changed_at: None,
            model_label: None,
        }]
    );
    assert_eq!(
        activities[0].presentation_phase().accessibility_label(),
        "Blocked"
    );
}

#[test]
fn empty_or_passive_oz_is_ignored() {
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::InProgress,
            is_empty: true,
            is_entirely_passive: false,
            is_ambient: false,
            phase_changed_at: None,
            model_label: None,
        }),
        false,
    );
    assert!(activities.is_empty());
}

#[test]
fn entirely_passive_oz_in_progress_is_ignored() {
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::InProgress,
            is_empty: false,
            is_entirely_passive: true,
            is_ambient: false,
            phase_changed_at: None,
            model_label: None,
        }),
        false,
    );
    assert!(activities.is_empty());
}

#[test]
fn oz_error_keeps_failed_residual() {
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::Error,
            is_empty: false,
            is_entirely_passive: false,
            is_ambient: false,
            phase_changed_at: None,
            model_label: None,
        }),
        false,
    );
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Oz { ambient: false },
            phase: WorkspaceAgentPhase::Failed,
            phase_changed_at: None,
            model_label: None,
        }]
    );
}

#[test]
fn oz_cancelled_becomes_idle_residual() {
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::Cancelled,
            is_empty: false,
            is_entirely_passive: false,
            is_ambient: false,
            phase_changed_at: None,
            model_label: None,
        }),
        false,
    );
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Oz { ambient: false },
            phase: WorkspaceAgentPhase::Idle,
            phase_changed_at: None,
            model_label: None,
        }]
    );
}

#[test]
fn cli_wins_over_oz_on_the_same_terminal() {
    let activities = activities_from_terminal_sources(
        Some((CLIAgent::Grok, CLIAgentSessionStatus::InProgress, None, None)),
        Some(OzConversationSource {
            status: ConversationStatus::Blocked {
                blocked_action: "ask".to_string(),
            },
            is_empty: false,
            is_entirely_passive: false,
            is_ambient: false,
            phase_changed_at: None,
            model_label: None,
        }),
        false,
    );
    assert_eq!(last_agent_activity(activities), Some(grok_running()));
}

#[test]
fn ambient_in_progress_uses_oz_cloud_identity() {
    let activities = activities_from_terminal_sources(None, None, true);
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Oz { ambient: true },
            phase: WorkspaceAgentPhase::InProgress,
            phase_changed_at: None,
            model_label: None,
        }]
    );
}

#[test]
fn ambient_oz_source_uses_oz_cloud_identity() {
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::InProgress,
            is_empty: false,
            is_entirely_passive: false,
            is_ambient: true,
            phase_changed_at: None,
            model_label: None,
        }),
        false,
    );
    assert_eq!(
        activities,
        vec![WorkspaceAgentActivity {
            identity: WorkspaceAgentIdentity::Oz { ambient: true },
            phase: WorkspaceAgentPhase::InProgress,
            phase_changed_at: None,
            model_label: None,
        }]
    );
}

#[test]
fn relative_time_label_formats_phase_changed_at() {
    let activity = WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
        phase: WorkspaceAgentPhase::Done,
        phase_changed_at: Some(instant::Instant::now()),
        model_label: None,
    };
    assert_eq!(activity.relative_time_label().as_deref(), Some("now"));
}

#[test]
fn cli_phase_changed_at_is_forwarded() {
    let at = instant::Instant::now();
    let activities = activities_from_terminal_sources(
        Some((CLIAgent::Grok, CLIAgentSessionStatus::Success, Some(at), None)),
        None,
        false,
    );
    assert_eq!(activities.len(), 1);
    assert_eq!(activities[0].phase, WorkspaceAgentPhase::Done);
    assert_eq!(activities[0].phase_changed_at, Some(at));
}

#[test]
fn oz_phase_changed_at_is_forwarded() {
    let at = instant::Instant::now();
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::Success,
            is_empty: false,
            is_entirely_passive: false,
            is_ambient: false,
            phase_changed_at: Some(at),
            model_label: None,
        }),
        false,
    );
    assert_eq!(activities.len(), 1);
    assert_eq!(activities[0].phase, WorkspaceAgentPhase::Done);
    assert_eq!(activities[0].phase_changed_at, Some(at));
}

#[test]
fn model_chip_label_hides_empty_and_shortens_path() {
    assert_eq!(model_chip_label(None), None);
    assert_eq!(model_chip_label(Some("")), None);
    assert_eq!(model_chip_label(Some("   ")), None);
    assert_eq!(
        model_chip_label(Some("anthropic/claude-opus-4")).as_deref(),
        Some("claude-opus-4")
    );
    assert_eq!(model_chip_label(Some("opus")).as_deref(), Some("opus"));
}

#[test]
fn cli_model_label_is_forwarded() {
    let activities = activities_from_terminal_sources(
        Some((
            CLIAgent::Claude,
            CLIAgentSessionStatus::InProgress,
            None,
            Some("opus".to_owned()),
        )),
        None,
        false,
    );
    assert_eq!(activities[0].model_chip_label().as_deref(), Some("opus"));
    assert_eq!(activities[0].model_chip_tooltip(), Some("opus"));
}

#[test]
fn oz_model_label_is_forwarded() {
    let activities = activities_from_terminal_sources(
        None,
        Some(OzConversationSource {
            status: ConversationStatus::InProgress,
            is_empty: false,
            is_entirely_passive: false,
            is_ambient: false,
            phase_changed_at: None,
            model_label: Some("gpt-5".to_owned()),
        }),
        false,
    );
    assert_eq!(activities[0].model_chip_label().as_deref(), Some("gpt-5"));
    assert_eq!(activities[0].model_chip_tooltip(), Some("gpt-5"));
}

#[test]
fn model_chip_tooltip_prefers_fuller_raw_when_shortened() {
    let shortened = WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Claude),
        phase: WorkspaceAgentPhase::InProgress,
        phase_changed_at: None,
        model_label: Some("anthropic/claude-opus-4".to_owned()),
    };
    assert_eq!(shortened.model_chip_label().as_deref(), Some("claude-opus-4"));
    assert_eq!(
        shortened.model_chip_tooltip(),
        Some("anthropic/claude-opus-4")
    );

    let plain = WorkspaceAgentActivity {
        model_label: Some("opus".to_owned()),
        ..shortened.clone()
    };
    assert_eq!(plain.model_chip_label().as_deref(), Some("opus"));
    // Orca still titles with the model string even when it matches the chip.
    assert_eq!(plain.model_chip_tooltip(), Some("opus"));

    let empty = WorkspaceAgentActivity {
        model_label: None,
        ..shortened
    };
    assert_eq!(empty.model_chip_label(), None);
    assert_eq!(empty.model_chip_tooltip(), None);
}

#[test]
fn done_just_under_24h_stays_done() {
    let changed_at = Instant::now();
    let now = changed_at + DONE_RESIDUAL_TTL - Duration::from_secs(1);
    assert_eq!(
        demote_done_phase(WorkspaceAgentPhase::Done, Some(changed_at), now),
        WorkspaceAgentPhase::Done
    );
    let activity = WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
        phase: WorkspaceAgentPhase::Done,
        phase_changed_at: Some(changed_at),
        model_label: None,
    };
    assert_eq!(
        activity.presentation_phase_at(now),
        WorkspaceAgentPhase::Done
    );
    assert!(activity.duration_until_done_demotion_at(now).is_some());
}

#[test]
fn done_at_24h_demotes_to_idle() {
    let changed_at = Instant::now();
    let now = changed_at + DONE_RESIDUAL_TTL;
    assert_eq!(
        demote_done_phase(WorkspaceAgentPhase::Done, Some(changed_at), now),
        WorkspaceAgentPhase::Idle
    );
    let mut activity = WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Grok),
        phase: WorkspaceAgentPhase::Done,
        phase_changed_at: Some(changed_at),
        model_label: None,
    };
    assert_eq!(
        activity.presentation_phase_at(now),
        WorkspaceAgentPhase::Idle
    );
    assert!(activity.duration_until_done_demotion_at(now).is_none());
    activity.apply_done_demotion_at(now);
    assert_eq!(activity.phase, WorkspaceAgentPhase::Idle);
    // Relative-time anchor stays at Done entry so age keeps growing (`1d`…).
    assert_eq!(activity.phase_changed_at, Some(changed_at));
}

#[test]
fn done_over_24h_demotes_to_idle() {
    let changed_at = Instant::now();
    let now = changed_at + DONE_RESIDUAL_TTL + Duration::from_secs(60);
    assert_eq!(
        demote_done_phase(WorkspaceAgentPhase::Done, Some(changed_at), now),
        WorkspaceAgentPhase::Idle
    );
}

#[test]
fn done_without_timestamp_stays_done() {
    assert_eq!(
        demote_done_phase(WorkspaceAgentPhase::Done, None, Instant::now()),
        WorkspaceAgentPhase::Done
    );
    assert!(duration_until_done_demotion(
        WorkspaceAgentPhase::Done,
        None,
        Instant::now()
    )
    .is_none());
}

#[test]
fn demoted_idle_does_not_count_as_busy_for_green_dot() {
    let changed_at = Instant::now();
    let now = changed_at + DONE_RESIDUAL_TTL;
    let mut activity = WorkspaceAgentActivity {
        identity: WorkspaceAgentIdentity::Cli(CLIAgent::Claude),
        phase: WorkspaceAgentPhase::Done,
        phase_changed_at: Some(changed_at),
        model_label: None,
    };
    assert_eq!(
        activity.presentation_phase_at(now),
        WorkspaceAgentPhase::Idle
    );
    assert!(!activity.presentation_phase_at(now).counts_as_busy());
    activity.apply_done_demotion_at(now);
    // After demotion the stored phase is Idle — still must not light collapsed green.
    assert_eq!(activity.phase, WorkspaceAgentPhase::Idle);
    assert!(!activity.counts_as_busy());
}

#[test]
fn non_done_phases_are_not_demoted() {
    let changed_at = Instant::now();
    let now = changed_at + DONE_RESIDUAL_TTL + Duration::from_secs(1);
    for phase in [
        WorkspaceAgentPhase::Idle,
        WorkspaceAgentPhase::InProgress,
        WorkspaceAgentPhase::Blocked,
        WorkspaceAgentPhase::Failed,
        WorkspaceAgentPhase::Waiting,
    ] {
        assert_eq!(demote_done_phase(phase, Some(changed_at), now), phase);
    }
}

