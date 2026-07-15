use std::path::PathBuf;

use crate::project_organization::domain::{RepositoryId, RepositoryWorkspaceId};
use crate::project_organization::git::BranchRef;

use super::{
    branch_ref_options, default_worktree_path, submit_is_disabled, CreateWorkspaceDefaults,
    CreateWorkspaceForm, CreateWorkspaceModalEvent, CreateWorkspaceMode, CreateWorkspaceSource,
    CreateWorkspaceTarget, RemoteBranchOption,
};

#[test]
fn remote_fetch_error_disables_submit_only_in_remote_mode() {
    assert!(submit_is_disabled(CreateWorkspaceMode::RemoteBranch, true));
    assert!(!submit_is_disabled(
        CreateWorkspaceMode::ExistingLocalBranch,
        true
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
