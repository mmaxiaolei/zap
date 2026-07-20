use std::path::PathBuf;

use crate::project_organization::domain::{RepositoryId, RepositoryWorkspaceId};
use crate::project_organization::git::{BranchRef, ExistingWorktreeOption, WorktreeInfo};

use super::{
    CreateWorkspaceDefaults, CreateWorkspaceForm, CreateWorkspaceModalEvent, CreateWorkspaceMode,
    CreateWorkspaceSource, CreateWorkspaceTarget, RemoteBranchOption, branch_ref_options,
    default_worktree_path, existing_worktree_default_name, existing_worktree_display_label,
    primary_worktree_error, submit_is_disabled,
};

#[test]
fn primary_existing_worktree_uses_local_label_and_name() {
    let option = ExistingWorktreeOption::primary(PathBuf::from("/repo"), "main");

    assert_eq!(existing_worktree_display_label(&option), "main (local)");
    assert_eq!(existing_worktree_default_name(&option), "local");
}

#[test]
fn detached_primary_worktree_warning_keeps_linked_worktree_available() {
    let repository_root = PathBuf::from("/repo");
    let linked_path = PathBuf::from("/repo-feature");
    let worktrees = vec![
        WorktreeInfo {
            path: repository_root.clone(),
            head: Some("a".to_string()),
            branch: None,
            is_bare: false,
            is_detached: true,
            is_locked: false,
            locked_reason: None,
            is_prunable: false,
            prunable_reason: None,
        },
        WorktreeInfo {
            path: linked_path.clone(),
            head: Some("b".to_string()),
            branch: Some("refs/heads/feature/existing".to_string()),
            is_bare: false,
            is_detached: false,
            is_locked: false,
            locked_reason: None,
            is_prunable: false,
            prunable_reason: None,
        },
    ];

    assert!(primary_worktree_error(&repository_root, &worktrees).is_some());
    assert_eq!(
        super::existing_worktree_options(&repository_root, worktrees),
        vec![ExistingWorktreeOption::new(linked_path, "feature/existing")],
    );
}

#[test]
fn remote_fetch_error_disables_submit_only_in_remote_mode() {
    assert!(submit_is_disabled(
        CreateWorkspaceMode::RemoteBranch,
        true,
        false,
        false,
    ));
    assert!(!submit_is_disabled(
        CreateWorkspaceMode::RemoteBranch,
        false,
        true,
        false,
    ));
    assert!(!submit_is_disabled(
        CreateWorkspaceMode::ExistingLocalBranch,
        true,
        true,
        false,
    ));
}

#[test]
fn existing_worktree_submit_is_disabled_until_a_selection_is_available() {
    assert!(submit_is_disabled(
        CreateWorkspaceMode::ExistingWorktree,
        false,
        false,
        false,
    ));
    assert!(submit_is_disabled(
        CreateWorkspaceMode::ExistingWorktree,
        false,
        true,
        true,
    ));
    assert!(!submit_is_disabled(
        CreateWorkspaceMode::ExistingWorktree,
        false,
        false,
        true,
    ));
}

#[test]
fn retry_event_targets_the_configured_workspace() {
    let target = CreateWorkspaceTarget {
        repository_id: RepositoryId(uuid::Uuid::from_u128(1)),
        workspace_id: RepositoryWorkspaceId(uuid::Uuid::from_u128(2)),
    };

    assert_eq!(
        target.retry_branch_refs_event(),
        CreateWorkspaceModalEvent::RetryBranchRefs {
            repository_id: RepositoryId(uuid::Uuid::from_u128(1)),
            workspace_id: RepositoryWorkspaceId(uuid::Uuid::from_u128(2)),
        },
    );
}

#[test]
fn selecting_remote_branch_overwrites_all_derived_workspace_fields() {
    let mut defaults =
        CreateWorkspaceDefaults::new(PathBuf::from("/Users/example"), "dip-agent".to_string());
    defaults.apply_branch("feature/one");
    defaults.new_branch = "custom".to_string();
    defaults.workspace_name = "custom workspace".to_string();
    defaults.worktree_path = PathBuf::from("/tmp/custom");

    defaults.apply_branch("feature/two");

    assert_eq!(defaults.new_branch, "feature/two");
    assert_eq!(defaults.workspace_name, "feature/two");
    assert_eq!(
        defaults.worktree_path,
        PathBuf::from("/Users/example/.warp/worktrees/dip-agent/feature-two")
    );
}

#[test]
fn switching_workspace_creation_modes_clears_incompatible_branch_selection() {
    let mut form = CreateWorkspaceForm::new();
    form.set_remote_ref("refs/remotes/origin/main".to_string());
    form.set_new_branch("feature/project-tree".to_string());

    form.set_mode(CreateWorkspaceMode::ExistingLocalBranch);

    assert_eq!(form.remote_ref(), None);
    assert_eq!(form.new_branch(), "");
    assert_eq!(form.mode(), CreateWorkspaceMode::ExistingLocalBranch);
}

#[test]
fn local_branch_mode_rejects_remote_refs() {
    let mut form = CreateWorkspaceForm::new();
    form.set_mode(CreateWorkspaceMode::ExistingLocalBranch);
    form.set_local_branch("refs/remotes/origin/main".to_string());

    assert!(!form.can_submit());

    form.set_local_branch("feature/project-tree".to_string());
    assert!(form.can_submit());
}

#[test]
fn remote_branch_form_builds_a_workspace_creation_request() {
    let repository_id = RepositoryId(uuid::Uuid::from_u128(1));
    let workspace_id = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut form = CreateWorkspaceForm::new();
    form.set_remote_ref("refs/remotes/origin/main".to_string());
    form.set_new_branch("feature/project-tree".to_string());

    let request = form
        .build_request(
            repository_id,
            workspace_id,
            "Project tree".to_string(),
            PathBuf::from("/tmp/project-tree"),
        )
        .unwrap();

    assert_eq!(request.repository_id, repository_id);
    assert_eq!(request.workspace_id, workspace_id);
    assert_eq!(request.display_name, "Project tree");
    assert_eq!(request.worktree_path, PathBuf::from("/tmp/project-tree"));
    assert!(matches!(
        request.source,
        CreateWorkspaceSource::RemoteBranch {
            remote_ref,
            new_branch,
        } if remote_ref == "refs/remotes/origin/main" && new_branch == "feature/project-tree"
    ));
}

#[test]
fn existing_worktree_form_builds_a_workspace_creation_request() {
    let repository_id = RepositoryId(uuid::Uuid::from_u128(1));
    let workspace_id = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut form = CreateWorkspaceForm::new();
    form.set_mode(CreateWorkspaceMode::ExistingWorktree);
    form.set_existing_worktree_branch("feature/adopt".to_string());

    let request = form
        .build_request(
            repository_id,
            workspace_id,
            "Adopt workspace".to_string(),
            PathBuf::from("/tmp/adopt"),
        )
        .unwrap();

    assert_eq!(request.repository_id, repository_id);
    assert_eq!(request.workspace_id, workspace_id);
    assert_eq!(request.display_name, "Adopt workspace");
    assert_eq!(request.worktree_path, PathBuf::from("/tmp/adopt"));
    assert!(matches!(
        request.source,
        CreateWorkspaceSource::ExistingWorktree { local_branch }
            if local_branch == "feature/adopt"
    ));
}

#[test]
fn remote_branch_options_hide_ref_prefix_and_disambiguate_duplicate_names() {
    let (remote_options, local_branches) = branch_ref_options([
        BranchRef::Remote {
            remote: "origin".to_string(),
            name: "main".to_string(),
            full_ref: "refs/remotes/origin/main".to_string(),
        },
        BranchRef::Remote {
            remote: "upstream".to_string(),
            name: "main".to_string(),
            full_ref: "refs/remotes/upstream/main".to_string(),
        },
        BranchRef::Remote {
            remote: "origin".to_string(),
            name: "feature/tree".to_string(),
            full_ref: "refs/remotes/origin/feature/tree".to_string(),
        },
        BranchRef::Local {
            name: "feature/local".to_string(),
            full_ref: "refs/heads/feature/local".to_string(),
        },
    ]);

    assert_eq!(
        remote_options,
        vec![
            RemoteBranchOption::new(
                "refs/remotes/origin/feature/tree",
                "origin",
                "feature/tree",
                "feature/tree",
            ),
            RemoteBranchOption::new("refs/remotes/origin/main", "origin", "main", "origin/main",),
            RemoteBranchOption::new(
                "refs/remotes/upstream/main",
                "upstream",
                "main",
                "upstream/main",
            ),
        ]
    );
    assert_eq!(local_branches, vec!["feature/local"]);
}

#[test]
fn default_worktree_path_uses_repository_and_branch_names() {
    assert_eq!(
        default_worktree_path(
            PathBuf::from("/Users/example"),
            "dip-agent",
            "feature/project-tree",
        ),
        PathBuf::from("/Users/example/.warp/worktrees/dip-agent/feature-project-tree"),
    );
}
