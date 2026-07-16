use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use pathfinder_geometry::vector::vec2f;
use warp_core::ui::appearance::Appearance;
use warpui::{
    elements::{ChildView, MouseStateHandle, ParentElement, Stack},
    platform::WindowStyle,
    App, Element, Entity, Event, Presenter, TypedActionView, View, ViewContext, ViewHandle,
    WindowInvalidation,
};

use crate::{
    persistence::{model::Repository as PersistedRepository, RepositoryPersistence},
    project_organization::model::ProjectOrganizationModel,
};

use crate::project_organization::domain::{
    Repository, RepositoryId, RepositorySource, RepositoryWorkspace, RepositoryWorkspaceId,
};

use super::{
    repository_add_workspace_position_id, resolved_project_organization_tab_layout,
    should_show_workspace_delete_button, synchronize_mouse_states, ProjectTreeEvent,
    ProjectTreePanel, ProjectTreeState, RepositoryTreeNode, TabLayout, WorkspaceTreeNode,
};

struct ProjectTreeTestHost {
    project_tree: ViewHandle<ProjectTreePanel>,
    events: Rc<RefCell<Vec<ProjectTreeEvent>>>,
}

impl ProjectTreeTestHost {
    fn new(ctx: &mut ViewContext<Self>) -> Self {
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let project_tree = ctx.add_typed_action_view(ProjectTreePanel::new);
        ctx.subscribe_to_view(&project_tree, move |_, _, event, _| {
            captured_events.borrow_mut().push(event.clone());
        });
        Self {
            project_tree,
            events,
        }
    }
}

impl Entity for ProjectTreeTestHost {
    type Event = ();
}

impl View for ProjectTreeTestHost {
    fn ui_name() -> &'static str {
        "ProjectTreeTestHost"
    }

    fn render(&self, _app: &warpui::AppContext) -> Box<dyn warpui::elements::Element> {
        Stack::new()
            .with_child(ChildView::new(&self.project_tree).finish())
            .finish()
    }
}

impl TypedActionView for ProjectTreeTestHost {
    type Action = ();
}

#[test]
fn tree_renders_two_levels_and_selects_workspace() {
    let repository_id = RepositoryId(uuid::Uuid::from_u128(1));
    let workspace_id = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut state = ProjectTreeState::new(vec![RepositoryTreeNode {
        repository_id,
        display_name: "zap".to_string(),
        expanded: true,
        workspaces: vec![WorkspaceTreeNode {
            workspace_id,
            display_name: "Feature A".to_string(),
            branch: "feature/a".to_string(),
            tab_count: 2,
        }],
    }]);

    assert_eq!(state.visible_rows().len(), 2);
    state.select_workspace(workspace_id);
    assert_eq!(state.selected_workspace_id(), Some(workspace_id));
}

#[test]
fn repository_workspace_mode_keeps_setting_but_uses_horizontal_tabbar() {
    assert_eq!(
        resolved_project_organization_tab_layout(true, true),
        TabLayout::Horizontal
    );
    assert_eq!(
        resolved_project_organization_tab_layout(false, true),
        TabLayout::Vertical
    );
}

#[test]
fn tree_groups_and_sorts_workspaces_by_repository() {
    let repository_a = RepositoryId(uuid::Uuid::from_u128(1));
    let repository_b = RepositoryId(uuid::Uuid::from_u128(2));
    let workspace_a = RepositoryWorkspaceId(uuid::Uuid::from_u128(3));
    let workspace_b = RepositoryWorkspaceId(uuid::Uuid::from_u128(4));
    let timestamp = chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc();

    let state = ProjectTreeState::from_records(
        vec![
            Repository {
                id: repository_b,
                display_name: "zebra".to_string(),
                path: "/tmp/zebra".into(),
                remote_url: None,
                source: RepositorySource::Local,
                created_at: timestamp,
                last_opened_at: timestamp,
            },
            Repository {
                id: repository_a,
                display_name: "alpha".to_string(),
                path: "/tmp/alpha".into(),
                remote_url: None,
                source: RepositorySource::Local,
                created_at: timestamp,
                last_opened_at: timestamp,
            },
        ],
        vec![
            RepositoryWorkspace {
                id: workspace_a,
                repository_id: repository_a,
                display_name: "zeta".to_string(),
                branch: "feature/zeta".to_string(),
                worktree_path: "/tmp/alpha-zeta".into(),
                created_at: timestamp,
                last_opened_at: timestamp,
            },
            RepositoryWorkspace {
                id: workspace_b,
                repository_id: repository_a,
                display_name: "beta".to_string(),
                branch: "feature/beta".to_string(),
                worktree_path: "/tmp/alpha-beta".into(),
                created_at: timestamp,
                last_opened_at: timestamp,
            },
        ],
        &[(workspace_a, 2), (workspace_b, 1)].into_iter().collect(),
    );

    assert_eq!(state.repositories()[0].display_name, "alpha");
    assert_eq!(state.repositories()[1].display_name, "zebra");
    assert_eq!(state.repositories()[0].workspaces[0].display_name, "beta");
    assert_eq!(state.repositories()[0].workspaces[0].tab_count, 1);
    assert_eq!(state.repositories()[0].workspaces[1].display_name, "zeta");
    assert_eq!(state.repositories()[0].workspaces[1].tab_count, 2);
}

#[test]
fn tree_can_clear_and_restore_the_active_workspace_selection() {
    let repository_id = RepositoryId(uuid::Uuid::from_u128(1));
    let workspace_id = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut state = ProjectTreeState::new(vec![RepositoryTreeNode {
        repository_id,
        display_name: "zap".to_string(),
        expanded: true,
        workspaces: vec![WorkspaceTreeNode {
            workspace_id,
            display_name: "Feature A".to_string(),
            branch: "feature/a".to_string(),
            tab_count: 0,
        }],
    }]);

    state.set_active_workspace(Some(workspace_id));
    assert_eq!(state.selected_workspace_id(), Some(workspace_id));

    state.set_active_workspace(None);
    assert_eq!(state.selected_workspace_id(), None);
}

#[test]
fn synchronize_mouse_states_removes_stale_entries_and_preserves_existing_handles() {
    let retained_id = RepositoryId(uuid::Uuid::from_u128(1));
    let stale_id = RepositoryId(uuid::Uuid::from_u128(2));
    let retained_handle = MouseStateHandle::default();
    let mut mouse_states = HashMap::from([
        (retained_id, retained_handle.clone()),
        (stale_id, MouseStateHandle::default()),
    ]);

    synchronize_mouse_states(&mut mouse_states, &HashSet::from([retained_id]));

    assert_eq!(mouse_states.len(), 1);
    assert!(!mouse_states.contains_key(&stale_id));
    assert!(Arc::ptr_eq(
        &retained_handle,
        mouse_states
            .get(&retained_id)
            .expect("retained mouse state should remain cached"),
    ));
}

#[test]
fn workspace_delete_button_only_shows_when_workspace_row_is_hovered() {
    assert!(!should_show_workspace_delete_button(false));
    assert!(should_show_workspace_delete_button(true));
}

#[test]
fn create_workspace_button_does_not_toggle_its_repository() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        let tempdir = tempfile::tempdir().expect("temporary directory should be created");
        let repository_path = tempdir.path().join("dip-agent");
        std::fs::create_dir(&repository_path).expect("repository directory should be created");
        let repository_id = RepositoryId(uuid::Uuid::from_u128(1));
        let timestamp = chrono::DateTime::from_timestamp(0, 0)
            .expect("timestamp should be valid")
            .naive_utc();
        app.add_singleton_model(|ctx| {
            ProjectOrganizationModel::try_new(
                vec![PersistedRepository {
                    id: repository_id.to_string(),
                    display_name: "dip-agent".to_string(),
                    path: repository_path.to_string_lossy().to_string(),
                    remote_url: None,
                    source: "local".to_string(),
                    created_at: timestamp,
                    last_opened_at: timestamp,
                }],
                vec![],
                RepositoryPersistence::new(None),
                ctx,
            )
            .expect("project organization model should initialize")
        });

        let (window_id, host) =
            app.add_window(WindowStyle::NotStealFocus, ProjectTreeTestHost::new);
        let (project_tree, events) = host.read(&app, |host, _| {
            (host.project_tree.clone(), host.events.clone())
        });
        let root_view_id = app
            .root_view_id(window_id)
            .expect("window should have a root view");
        let mut presenter = Presenter::new(window_id);
        let invalidation = WindowInvalidation {
            updated: [root_view_id, project_tree.id()].into_iter().collect(),
            ..Default::default()
        };

        app.update(|ctx| {
            presenter.invalidate(invalidation, ctx);
            presenter.build_scene(vec2f(320., 160.), 1., None, ctx);
            let button_position_id = repository_add_workspace_position_id(repository_id);
            let click_position = presenter
                .position_cache()
                .get_position(&button_position_id)
                .expect("create workspace button should have a saved position")
                .center();
            let presenter = Rc::new(RefCell::new(presenter));
            ctx.simulate_window_event(
                Event::LeftMouseDown {
                    position: click_position,
                    modifiers: Default::default(),
                    click_count: 1,
                    is_first_mouse: false,
                },
                window_id,
                presenter.clone(),
            );
            presenter.borrow_mut().invalidate(
                WindowInvalidation {
                    updated: [root_view_id, project_tree.id()].into_iter().collect(),
                    ..Default::default()
                },
                ctx,
            );
            presenter
                .borrow_mut()
                .build_scene(vec2f(320., 160.), 1., None, ctx);
            ctx.simulate_window_event(
                Event::LeftMouseUp {
                    position: click_position,
                    modifiers: Default::default(),
                },
                window_id,
                presenter,
            );
        });

        assert!(matches!(
            events.borrow().as_slice(),
            [ProjectTreeEvent::CreateWorkspaceRequested {
                repository_id: event_repository_id
            }] if *event_repository_id == repository_id
        ));
        project_tree.read(&app, |project_tree, _| {
            assert!(project_tree.state.repositories()[0].expanded);
        });
    });
}
