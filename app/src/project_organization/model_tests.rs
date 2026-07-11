use std::{
    path::Path,
    sync::mpsc::{self, Receiver},
};

use chrono::{Duration, Utc};
use tempfile::TempDir;
use uuid::Uuid;
use warpui::{App, ModelHandle};

use crate::{
    persistence::{
        model::{Repository as PersistedRepository, RepositoryWorkspace as PersistedWorkspace},
        ModelEvent,
    },
    project_organization::{
        domain::{
            ProjectOrganizationError, RepositoryId, RepositorySource, RepositoryWorkspace,
            RepositoryWorkspaceId,
        },
        model::ProjectOrganizationModel,
    },
};

fn create_model(
    app: &mut App,
    repositories: Vec<PersistedRepository>,
    workspaces: Vec<PersistedWorkspace>,
) -> (ModelHandle<ProjectOrganizationModel>, Receiver<ModelEvent>) {
    let (sender, receiver) = mpsc::sync_channel(20);
    let model = app.add_model(|ctx| {
        ProjectOrganizationModel::try_new(repositories, workspaces, Some(sender), ctx)
            .expect("project organization model should initialize")
    });
    (model, receiver)
}

fn persisted_repository(id: RepositoryId, path: &Path) -> PersistedRepository {
    let created_at = Utc::now().naive_utc() - Duration::hours(1);
    PersistedRepository {
        id: id.to_string(),
        display_name: "repository".to_string(),
        path: path
            .to_str()
            .expect("temporary repository path should be valid UTF-8")
            .to_string(),
        remote_url: None,
        source: "local".to_string(),
        created_at,
        last_opened_at: created_at,
    }
}

fn repository_workspace(
    id: RepositoryWorkspaceId,
    repository_id: RepositoryId,
    branch: &str,
    worktree_path: &Path,
) -> RepositoryWorkspace {
    let created_at = Utc::now().naive_utc() - Duration::minutes(30);
    RepositoryWorkspace {
        id,
        repository_id,
        display_name: branch.to_string(),
        branch: branch.to_string(),
        worktree_path: worktree_path.to_path_buf(),
        created_at,
        last_opened_at: created_at,
    }
}

#[test]
fn add_local_repository_rejects_duplicate_canonical_path() {
    App::test((), |mut app| async move {
        let tempdir = TempDir::new().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("repository");
        std::fs::create_dir(&repository_path).expect("repository directory should be created");
        let alias_path = repository_path.join("..").join("repository");
        let canonical_path =
            dunce::canonicalize(&repository_path).expect("repository path should canonicalize");
        let (model, _events) = create_model(&mut app, vec![], vec![]);

        let repository_id = model
            .update(&mut app, |model, ctx| {
                model.add_local_repository(&repository_path, ctx)
            })
            .expect("first repository should be added");
        let error = model
            .update(&mut app, |model, ctx| {
                model.add_local_repository(&alias_path, ctx)
            })
            .expect_err("canonical duplicate should be rejected");

        assert!(matches!(
            error,
            ProjectOrganizationError::RepositoryAlreadyExists {
                existing_repository_id,
                canonical_path: existing_path,
            } if existing_repository_id == repository_id && existing_path == canonical_path
        ));
    });
}

#[test]
fn insert_workspace_rejects_duplicate_repository_branch() {
    App::test((), |mut app| async move {
        let tempdir = TempDir::new().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("repository");
        let first_worktree = tempdir.path().join("first-worktree");
        let second_worktree = tempdir.path().join("second-worktree");
        for path in [&repository_path, &first_worktree, &second_worktree] {
            std::fs::create_dir(path).expect("test directory should be created");
        }
        let (model, _events) = create_model(&mut app, vec![], vec![]);
        let repository_id = model
            .update(&mut app, |model, ctx| {
                model.add_local_repository(&repository_path, ctx)
            })
            .expect("repository should be added");
        let first_workspace_id = RepositoryWorkspaceId::from(Uuid::new_v4());
        model
            .update(&mut app, |model, ctx| {
                model.insert_workspace(
                    repository_workspace(
                        first_workspace_id,
                        repository_id,
                        "feature/branch",
                        &first_worktree,
                    ),
                    ctx,
                )
            })
            .expect("first workspace should be inserted");

        let error = model
            .update(&mut app, |model, ctx| {
                model.insert_workspace(
                    repository_workspace(
                        RepositoryWorkspaceId::from(Uuid::new_v4()),
                        repository_id,
                        "feature/branch",
                        &second_worktree,
                    ),
                    ctx,
                )
            })
            .expect_err("duplicate branch should be rejected");

        assert!(matches!(
            error,
            ProjectOrganizationError::WorkspaceBranchAlreadyExists {
                repository_id: duplicate_repository_id,
                branch,
                existing_workspace_id,
            } if duplicate_repository_id == repository_id
                && branch == "feature/branch"
                && existing_workspace_id == first_workspace_id
        ));
    });
}

#[test]
fn remove_repository_is_blocked_while_workspace_exists() {
    App::test((), |mut app| async move {
        let tempdir = TempDir::new().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("repository");
        let worktree_path = tempdir.path().join("worktree");
        std::fs::create_dir(&repository_path).expect("repository directory should be created");
        std::fs::create_dir(&worktree_path).expect("worktree directory should be created");
        let (model, _events) = create_model(&mut app, vec![], vec![]);
        let repository_id = model
            .update(&mut app, |model, ctx| {
                model.add_local_repository(&repository_path, ctx)
            })
            .expect("repository should be added");
        let workspace_id = RepositoryWorkspaceId::from(Uuid::new_v4());
        model
            .update(&mut app, |model, ctx| {
                model.insert_workspace(
                    repository_workspace(workspace_id, repository_id, "main", &worktree_path),
                    ctx,
                )
            })
            .expect("workspace should be inserted");

        let error = model
            .update(&mut app, |model, ctx| {
                model.remove_repository(repository_id, ctx)
            })
            .expect_err("repository with workspaces should not be removed");

        assert!(matches!(
            error,
            ProjectOrganizationError::RepositoryHasWorkspaces {
                repository_id: blocked_repository_id,
            } if blocked_repository_id == repository_id
        ));
    });
}

#[test]
fn rename_repository_changes_only_display_name() {
    App::test((), |mut app| async move {
        let tempdir = TempDir::new().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("repository");
        std::fs::create_dir(&repository_path).expect("repository directory should be created");
        let canonical_path =
            dunce::canonicalize(&repository_path).expect("repository path should canonicalize");
        let (model, _events) = create_model(&mut app, vec![], vec![]);
        let repository_id = model
            .update(&mut app, |model, ctx| {
                model.add_local_repository(&repository_path, ctx)
            })
            .expect("repository should be added");

        model
            .update(&mut app, |model, ctx| {
                model.rename_repository(repository_id, "Renamed repository".to_string(), ctx)
            })
            .expect("repository should be renamed");
        let repository = model.read(&app, |model, _| {
            model
                .repository(repository_id)
                .expect("repository should exist")
                .clone()
        });

        assert_eq!(repository.display_name, "Renamed repository");
        assert_eq!(repository.path, canonical_path);
        assert_eq!(repository.source, RepositorySource::Local);
    });
}

#[test]
fn rename_workspace_changes_only_display_name() {
    App::test((), |mut app| async move {
        let tempdir = TempDir::new().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("repository");
        let worktree_path = tempdir.path().join("worktree");
        std::fs::create_dir(&repository_path).expect("repository directory should be created");
        std::fs::create_dir(&worktree_path).expect("worktree directory should be created");
        let canonical_worktree_path =
            dunce::canonicalize(&worktree_path).expect("worktree path should canonicalize");
        let (model, _events) = create_model(&mut app, vec![], vec![]);
        let repository_id = model
            .update(&mut app, |model, ctx| {
                model.add_local_repository(&repository_path, ctx)
            })
            .expect("repository should be added");
        let workspace_id = RepositoryWorkspaceId::from(Uuid::new_v4());
        model
            .update(&mut app, |model, ctx| {
                model.insert_workspace(
                    repository_workspace(
                        workspace_id,
                        repository_id,
                        "feature/branch",
                        &worktree_path,
                    ),
                    ctx,
                )
            })
            .expect("workspace should be inserted");

        model
            .update(&mut app, |model, ctx| {
                model.rename_workspace(workspace_id, "Renamed workspace".to_string(), ctx)
            })
            .expect("workspace should be renamed");
        let workspace = model.read(&app, |model, _| {
            model
                .workspace(workspace_id)
                .expect("workspace should exist")
                .clone()
        });

        assert_eq!(workspace.display_name, "Renamed workspace");
        assert_eq!(workspace.branch, "feature/branch");
        assert_eq!(workspace.worktree_path, canonical_worktree_path);
    });
}

#[test]
fn touch_repository_path_adds_repository_and_persistence_event() {
    App::test((), |mut app| async move {
        let tempdir = TempDir::new().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("repository");
        std::fs::create_dir(&repository_path).expect("repository directory should be created");
        let canonical_path =
            dunce::canonicalize(&repository_path).expect("repository path should canonicalize");
        let (model, events) = create_model(&mut app, vec![], vec![]);

        let repository_id = model
            .update(&mut app, |model, ctx| {
                model.touch_repository_path(&repository_path, ctx)
            })
            .expect("repository path should be touched");
        let event = events.recv().expect("persistence event should be sent");

        assert!(matches!(
            event,
            ModelEvent::UpsertRepository { repository }
                if repository.id == repository_id.to_string()
                    && repository.path == canonical_path.to_string_lossy()
        ));
    });
}

#[test]
fn touch_repository_path_updates_existing_timestamp_and_persistence_event() {
    App::test((), |mut app| async move {
        let tempdir = TempDir::new().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("repository");
        std::fs::create_dir(&repository_path).expect("repository directory should be created");
        let repository_id = RepositoryId::from(Uuid::new_v4());
        let persisted_repository = persisted_repository(repository_id, &repository_path);
        let previous_last_opened_at = persisted_repository.last_opened_at;
        let (model, events) = create_model(&mut app, vec![persisted_repository], vec![]);

        let touched_id = model
            .update(&mut app, |model, ctx| {
                model.touch_repository_path(&repository_path, ctx)
            })
            .expect("repository path should be touched");
        let event = events.recv().expect("persistence event should be sent");

        assert_eq!(touched_id, repository_id);
        assert!(matches!(
            event,
            ModelEvent::UpsertRepository { repository }
                if repository.id == repository_id.to_string()
                    && repository.last_opened_at > previous_last_opened_at
        ));
    });
}
