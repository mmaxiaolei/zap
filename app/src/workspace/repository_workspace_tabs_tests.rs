use crate::project_organization::domain::RepositoryWorkspaceId;

use super::repository_workspace_tabs::{RepositoryWorkspaceTabSets, RepositoryWorkspaceTabState};

#[test]
fn switching_workspaces_swaps_tabs_without_dropping_inactive_state() {
    let workspace_a = RepositoryWorkspaceId(uuid::Uuid::from_u128(1));
    let workspace_b = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut active_tabs = vec![10_u64];
    let mut active_tab_index = 0;
    let mut sets = RepositoryWorkspaceTabSets::new(Some(workspace_a));
    sets.insert_inactive(
        Some(workspace_b),
        RepositoryWorkspaceTabState::new(vec![20_u64], 0),
    );

    sets.switch_to(Some(workspace_b), &mut active_tabs, &mut active_tab_index);
    assert_eq!(active_tabs, vec![20]);

    sets.switch_to(Some(workspace_a), &mut active_tabs, &mut active_tab_index);
    assert_eq!(active_tabs, vec![10]);
}

#[test]
fn switching_workspaces_restores_each_workspace_active_tab_index() {
    let workspace_a = RepositoryWorkspaceId(uuid::Uuid::from_u128(1));
    let workspace_b = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut active_tabs = vec![10_u64, 11, 12];
    let mut active_tab_index = 2;
    let mut sets = RepositoryWorkspaceTabSets::new(Some(workspace_a));
    sets.insert_inactive(
        Some(workspace_b),
        RepositoryWorkspaceTabState::new(vec![20_u64], 0),
    );

    sets.switch_to(Some(workspace_b), &mut active_tabs, &mut active_tab_index);
    sets.switch_to(Some(workspace_a), &mut active_tabs, &mut active_tab_index);

    assert_eq!(active_tab_index, 2);
}

#[test]
fn tab_counts_include_active_and_inactive_workspaces() {
    let workspace_a = RepositoryWorkspaceId(uuid::Uuid::from_u128(1));
    let workspace_b = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut active_tabs = vec![10_u64, 11];
    let sets = {
        let mut sets = RepositoryWorkspaceTabSets::new(Some(workspace_a));
        sets.insert_inactive(
            Some(workspace_b),
            RepositoryWorkspaceTabState::new(vec![20_u64, 21, 22], 1),
        );
        sets.insert_inactive(None, RepositoryWorkspaceTabState::new(vec![30_u64], 0));
        sets
    };

    assert_eq!(sets.tab_counts(&active_tabs).get(&workspace_a), Some(&2));
    assert_eq!(sets.tab_counts(&active_tabs).get(&workspace_b), Some(&3));

    active_tabs.clear();
    assert_eq!(sets.tab_counts(&active_tabs).get(&workspace_a), None);
}

#[test]
fn taking_an_inactive_workspace_removes_only_its_tab_state() {
    let workspace_a = RepositoryWorkspaceId(uuid::Uuid::from_u128(1));
    let workspace_b = RepositoryWorkspaceId(uuid::Uuid::from_u128(2));
    let mut sets = RepositoryWorkspaceTabSets::new(Some(workspace_a));
    sets.insert_inactive(
        Some(workspace_b),
        RepositoryWorkspaceTabState::new(vec![20_u64], 0),
    );

    let removed = sets
        .take_inactive(Some(workspace_b))
        .expect("workspace state should be removed");

    assert_eq!(removed.tabs, vec![20]);
    assert!(sets.inactive_states().next().is_none());
    assert_eq!(sets.active_workspace_id(), Some(workspace_a));
}
