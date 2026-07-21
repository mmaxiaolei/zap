use std::collections::{HashMap, HashSet};

use crate::project_organization::domain::RepositoryWorkspaceId;

#[derive(Debug)]
pub(crate) struct RepositoryWorkspaceTabState<T> {
    pub(crate) tabs: Vec<T>,
    pub(crate) active_tab_index: usize,
}

impl<T> RepositoryWorkspaceTabState<T> {
    pub(crate) fn new(tabs: Vec<T>, active_tab_index: usize) -> Self {
        Self {
            active_tab_index: clamped_tab_index(active_tab_index, tabs.len()),
            tabs,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RepositoryWorkspaceTabSets<T> {
    active_workspace_id: Option<RepositoryWorkspaceId>,
    inactive: HashMap<Option<RepositoryWorkspaceId>, RepositoryWorkspaceTabState<T>>,
}

impl<T> RepositoryWorkspaceTabSets<T> {
    pub(crate) fn new(active_workspace_id: Option<RepositoryWorkspaceId>) -> Self {
        Self {
            active_workspace_id,
            inactive: HashMap::new(),
        }
    }

    pub(crate) fn active_workspace_id(&self) -> Option<RepositoryWorkspaceId> {
        self.active_workspace_id
    }

    pub(crate) fn insert_inactive(
        &mut self,
        workspace_id: Option<RepositoryWorkspaceId>,
        state: RepositoryWorkspaceTabState<T>,
    ) {
        debug_assert_ne!(workspace_id, self.active_workspace_id);
        self.inactive.insert(workspace_id, state);
    }

    pub(crate) fn take_inactive(
        &mut self,
        workspace_id: Option<RepositoryWorkspaceId>,
    ) -> Option<RepositoryWorkspaceTabState<T>> {
        self.inactive.remove(&workspace_id)
    }

    pub(crate) fn find_inactive_workspace(
        &self,
        mut contains: impl FnMut(&T) -> bool,
    ) -> Option<Option<RepositoryWorkspaceId>> {
        self.inactive.iter().find_map(|(workspace_id, state)| {
            state
                .tabs
                .iter()
                .any(&mut contains)
                .then_some(*workspace_id)
        })
    }

    pub(crate) fn workspace_ids_matching(
        &self,
        active_tabs: &[T],
        mut matches_tab: impl FnMut(&T) -> bool,
    ) -> HashSet<RepositoryWorkspaceId> {
        let mut workspace_ids = HashSet::new();

        if let Some(workspace_id) = self.active_workspace_id {
            if active_tabs.iter().any(&mut matches_tab) {
                workspace_ids.insert(workspace_id);
            }
        }

        for (workspace_id, state) in &self.inactive {
            let Some(workspace_id) = workspace_id else {
                continue;
            };
            if state.tabs.iter().any(&mut matches_tab) {
                workspace_ids.insert(*workspace_id);
            }
        }

        workspace_ids
    }

    pub(crate) fn switch_to(
        &mut self,
        workspace_id: Option<RepositoryWorkspaceId>,
        active_tabs: &mut Vec<T>,
        active_tab_index: &mut usize,
    ) {
        if workspace_id == self.active_workspace_id {
            *active_tab_index = clamped_tab_index(*active_tab_index, active_tabs.len());
            return;
        }

        let current = RepositoryWorkspaceTabState::new(
            std::mem::take(active_tabs),
            std::mem::take(active_tab_index),
        );
        self.inactive.insert(self.active_workspace_id, current);

        let next = self
            .inactive
            .remove(&workspace_id)
            .unwrap_or_else(|| RepositoryWorkspaceTabState::new(Vec::new(), 0));
        *active_tabs = next.tabs;
        *active_tab_index = next.active_tab_index;
        self.active_workspace_id = workspace_id;
    }

    pub(crate) fn inactive_states(
        &self,
    ) -> impl Iterator<
        Item = (
            &Option<RepositoryWorkspaceId>,
            &RepositoryWorkspaceTabState<T>,
        ),
    > {
        self.inactive.iter()
    }

    pub(crate) fn tab_counts(&self, active_tabs: &[T]) -> HashMap<RepositoryWorkspaceId, usize> {
        let mut counts = HashMap::new();
        if let Some(workspace_id) = self.active_workspace_id.filter(|_| !active_tabs.is_empty()) {
            counts.insert(workspace_id, active_tabs.len());
        }
        counts.extend(self.inactive.iter().filter_map(|(workspace_id, state)| {
            workspace_id
                .filter(|_| !state.tabs.is_empty())
                .map(|workspace_id| (workspace_id, state.tabs.len()))
        }));
        counts
    }
}

fn clamped_tab_index(index: usize, tab_count: usize) -> usize {
    tab_count
        .checked_sub(1)
        .map_or(0, |last_index| index.min(last_index))
}
