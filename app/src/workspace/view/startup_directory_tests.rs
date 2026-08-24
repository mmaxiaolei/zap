use super::startup_directory_with_repository_workspace_fallback;
use crate::terminal::session_settings::WorkingDirectoryMode;
use std::path::PathBuf;

#[test]
fn previous_dir_uses_worktree_when_no_prior_session() {
    assert_eq!(
        startup_directory_with_repository_workspace_fallback(
            None,
            WorkingDirectoryMode::PreviousDir,
            Some(PathBuf::from("/tmp/feature-worktree")),
        ),
        Some(PathBuf::from("/tmp/feature-worktree"))
    );
}

#[test]
fn previous_dir_keeps_inherited_session_directory() {
    assert_eq!(
        startup_directory_with_repository_workspace_fallback(
            Some(PathBuf::from("/tmp/current-session")),
            WorkingDirectoryMode::PreviousDir,
            Some(PathBuf::from("/tmp/feature-worktree")),
        ),
        Some(PathBuf::from("/tmp/current-session"))
    );
}

#[test]
fn home_dir_does_not_override_with_worktree() {
    assert_eq!(
        startup_directory_with_repository_workspace_fallback(
            None,
            WorkingDirectoryMode::HomeDir,
            Some(PathBuf::from("/tmp/feature-worktree")),
        ),
        None
    );
}

#[test]
fn custom_dir_does_not_override_with_worktree_when_settings_yield_none() {
    assert_eq!(
        startup_directory_with_repository_workspace_fallback(
            None,
            WorkingDirectoryMode::CustomDir,
            Some(PathBuf::from("/tmp/feature-worktree")),
        ),
        None
    );
}

#[test]
fn previous_dir_without_worktree_stays_none() {
    assert_eq!(
        startup_directory_with_repository_workspace_fallback(
            None,
            WorkingDirectoryMode::PreviousDir,
            None,
        ),
        None
    );
}
