use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use pathfinder_geometry::vector::vec2f;
use warp_core::ui::color::coloru_with_opacity;
use warpui::{
    elements::{
        Border, ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, DropShadow,
        Element, Empty, Flex, Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle,
        ParentElement, Radius, SavePosition, Shrinkable, Text,
    },
    platform::Cursor,
    text_layout::ClipConfig,
    ui_components::components::UiComponent,
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

use crate::{
    appearance::Appearance,
    project_organization::model::ProjectOrganizationModel,
    ui_components::{
        buttons::icon_button,
        icons,
        spinner::{BrailleSpinner, SpinnerStateHandle},
    },
    view_components::action_button::{ActionButton, ButtonSize, SecondaryTheme},
};

use crate::project_organization::domain::{
    Repository, RepositoryId, RepositoryWorkspace, RepositoryWorkspaceId,
};

const WORKSPACE_TAB_COUNT_BADGE_HEIGHT: f32 = 24.;
const WORKSPACE_TAB_COUNT_BADGE_SINGLE_DIGIT_WIDTH: f32 = 24.;
const WORKSPACE_TAB_COUNT_BADGE_WIDE_WIDTH: f32 = 30.;
const WORKSPACE_RUNNING_INDICATOR_SLOT_WIDTH: f32 = 16.;
const WORKSPACE_STATUS_SLOT_GAP: f32 = 6.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabLayout {
    Horizontal,
    Vertical,
}

/// 解析项目组织模式下的页签布局。
///
/// 启用 repository workspaces 时强制使用水平 TabBar, 但不会修改用户原有的
/// Vertical Tabs 设置值。
pub fn resolved_project_organization_tab_layout(
    repository_workspaces_enabled: bool,
    vertical_tabs_enabled: bool,
) -> TabLayout {
    if repository_workspaces_enabled || !vertical_tabs_enabled {
        TabLayout::Horizontal
    } else {
        TabLayout::Vertical
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceTreeNode {
    pub workspace_id: RepositoryWorkspaceId,
    pub display_name: String,
    pub branch: String,
    pub tab_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryTreeNode {
    pub repository_id: RepositoryId,
    pub display_name: String,
    pub expanded: bool,
    pub workspaces: Vec<WorkspaceTreeNode>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectTreeRow {
    Repository(RepositoryId),
    Workspace(RepositoryWorkspaceId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectTreeState {
    repositories: Vec<RepositoryTreeNode>,
    selected_workspace_id: Option<RepositoryWorkspaceId>,
}

impl ProjectTreeState {
    pub fn new(repositories: Vec<RepositoryTreeNode>) -> Self {
        Self {
            repositories,
            selected_workspace_id: None,
        }
    }

    pub fn repositories(&self) -> &[RepositoryTreeNode] {
        &self.repositories
    }

    pub fn from_records(
        repositories: Vec<Repository>,
        workspaces: Vec<RepositoryWorkspace>,
        tab_counts: &HashMap<RepositoryWorkspaceId, usize>,
    ) -> Self {
        let mut workspaces_by_repository = HashMap::<RepositoryId, Vec<RepositoryWorkspace>>::new();
        for workspace in workspaces {
            workspaces_by_repository
                .entry(workspace.repository_id)
                .or_default()
                .push(workspace);
        }

        let mut repositories = repositories;
        repositories.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.display_name.cmp(&right.display_name))
        });
        let repositories = repositories
            .into_iter()
            .map(|repository| {
                let mut workspaces = workspaces_by_repository
                    .remove(&repository.id)
                    .unwrap_or_default();
                workspaces.sort_by(|left, right| {
                    left.created_at
                        .cmp(&right.created_at)
                        .then_with(|| left.display_name.cmp(&right.display_name))
                });
                let workspaces = workspaces
                    .into_iter()
                    .map(|workspace| WorkspaceTreeNode {
                        workspace_id: workspace.id,
                        display_name: workspace.display_name,
                        branch: workspace.branch,
                        tab_count: tab_counts.get(&workspace.id).copied().unwrap_or_default(),
                    })
                    .collect();
                RepositoryTreeNode {
                    repository_id: repository.id,
                    display_name: repository.display_name,
                    expanded: true,
                    workspaces,
                }
            })
            .collect::<Vec<_>>();
        Self::new(repositories)
    }

    pub fn visible_rows(&self) -> Vec<ProjectTreeRow> {
        self.repositories
            .iter()
            .flat_map(|repository| {
                let repository_row =
                    std::iter::once(ProjectTreeRow::Repository(repository.repository_id));
                let workspace_rows = repository
                    .expanded
                    .then(|| {
                        repository
                            .workspaces
                            .iter()
                            .map(|workspace| ProjectTreeRow::Workspace(workspace.workspace_id))
                    })
                    .into_iter()
                    .flatten();
                repository_row.chain(workspace_rows)
            })
            .collect()
    }

    pub fn toggle_repository(&mut self, repository_id: RepositoryId) -> bool {
        let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|repository| repository.repository_id == repository_id)
        else {
            return false;
        };
        repository.expanded = !repository.expanded;
        true
    }

    pub fn select_workspace(&mut self, workspace_id: RepositoryWorkspaceId) -> bool {
        let exists = self.repositories.iter().any(|repository| {
            repository
                .workspaces
                .iter()
                .any(|workspace| workspace.workspace_id == workspace_id)
        });
        if exists {
            self.selected_workspace_id = Some(workspace_id);
        }
        exists
    }

    pub fn set_active_workspace(&mut self, workspace_id: Option<RepositoryWorkspaceId>) {
        if let Some(workspace_id) = workspace_id {
            if self.select_workspace(workspace_id) {
                return;
            }
        }
        self.selected_workspace_id = None;
    }

    pub fn selected_workspace_id(&self) -> Option<RepositoryWorkspaceId> {
        self.selected_workspace_id
    }
}

#[derive(Clone, Debug)]
pub enum ProjectTreeAction {
    AddRepository,
    CreateWorkspace {
        repository_id: RepositoryId,
    },
    DeleteWorkspace {
        workspace_id: RepositoryWorkspaceId,
    },
    ToggleRepository {
        repository_id: RepositoryId,
    },
    SelectWorkspace {
        workspace_id: Option<RepositoryWorkspaceId>,
    },
}

#[derive(Clone, Debug)]
pub enum ProjectTreeEvent {
    AddRepositoryRequested,
    CreateWorkspaceRequested {
        repository_id: RepositoryId,
    },
    DeleteWorkspaceRequested {
        workspace_id: RepositoryWorkspaceId,
    },
    WorkspaceSelected {
        workspace_id: Option<RepositoryWorkspaceId>,
    },
}

fn repository_add_workspace_position_id(repository_id: RepositoryId) -> String {
    format!("project_tree:repository:{repository_id}:add_workspace")
}

fn should_show_workspace_delete_button(workspace_row_hovered: bool) -> bool {
    workspace_row_hovered
}

fn workspace_row_is_selected(
    selected_workspace_id: Option<RepositoryWorkspaceId>,
    workspace_id: RepositoryWorkspaceId,
) -> bool {
    selected_workspace_id == Some(workspace_id)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceVisualState {
    is_selected: bool,
    has_running_terminal: bool,
}

impl WorkspaceVisualState {
    pub(crate) fn new(is_selected: bool, has_running_terminal: bool) -> Self {
        Self {
            is_selected,
            has_running_terminal,
        }
    }

    pub(crate) fn should_render_selection_frame(&self) -> bool {
        self.is_selected
    }

    pub(crate) fn should_render_running_spinner(&self) -> bool {
        self.has_running_terminal
    }
}

fn workspace_count_label(workspace_count: usize) -> String {
    let noun = if workspace_count == 1 {
        "workspace"
    } else {
        "workspaces"
    };
    format!("{workspace_count} {noun}")
}

fn tab_count_badge_label(tab_count: usize) -> String {
    if tab_count > 99 {
        "99+".to_string()
    } else {
        tab_count.to_string()
    }
}

fn workspace_tab_count_badge_width(tab_count: usize) -> f32 {
    if tab_count < 10 {
        WORKSPACE_TAB_COUNT_BADGE_SINGLE_DIGIT_WIDTH
    } else {
        WORKSPACE_TAB_COUNT_BADGE_WIDE_WIDTH
    }
}

fn apply_workspace_selection_frame(
    row_container: Container,
    visual_state: WorkspaceVisualState,
    selected_border_color: pathfinder_color::ColorU,
    selected_shadow_color: pathfinder_color::ColorU,
) -> Container {
    let row_container = row_container.with_border(Border::all(1.).with_border_fill(
        if visual_state.should_render_selection_frame() {
            selected_border_color
        } else {
            coloru_with_opacity(selected_border_color, 0)
        },
    ));
    if !visual_state.should_render_selection_frame() {
        return row_container;
    }

    row_container.with_drop_shadow(
        DropShadow::new_with_standard_offset_and_spread(selected_shadow_color)
            .with_offset(vec2f(0., 0.)),
    )
}

fn synchronize_mouse_states<Id>(mouse_states: &mut HashMap<Id, MouseStateHandle>, ids: &HashSet<Id>)
where
    Id: Copy + Eq + Hash,
{
    mouse_states.retain(|id, _| ids.contains(id));
    for id in ids {
        mouse_states.entry(*id).or_default();
    }
}

/// 左侧 repository/workspace 树。
///
/// 该视图只维护展示和选择状态。所有 Git、持久化和页签生命周期操作均通过
/// [`ProjectTreeEvent`] 交由窗口根处理，避免视图跨越领域边界。
pub struct ProjectTreePanel {
    project_organization_model: ModelHandle<ProjectOrganizationModel>,
    state: ProjectTreeState,
    tab_counts: HashMap<RepositoryWorkspaceId, usize>,
    running_workspace_ids: HashSet<RepositoryWorkspaceId>,
    workspace_spinner_states: HashMap<RepositoryWorkspaceId, SpinnerStateHandle>,
    repository_mouse_states: HashMap<RepositoryId, MouseStateHandle>,
    workspace_mouse_states: HashMap<RepositoryWorkspaceId, MouseStateHandle>,
    workspace_delete_mouse_states: HashMap<RepositoryWorkspaceId, MouseStateHandle>,
    repository_add_workspace_mouse_states: HashMap<RepositoryId, MouseStateHandle>,
    unclassified_mouse_state: MouseStateHandle,
    add_repository_button: ViewHandle<ActionButton>,
}

impl ProjectTreePanel {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let project_organization_model = ProjectOrganizationModel::handle(ctx);
        let add_repository_button = ctx.add_view(|_| {
            ActionButton::new("Add repository", SecondaryTheme)
                .with_icon(icons::Icon::Plus)
                .with_size(ButtonSize::Small)
                .on_click(|ctx| ctx.dispatch_typed_action(ProjectTreeAction::AddRepository))
        });

        let mut panel = Self {
            project_organization_model: project_organization_model.clone(),
            state: ProjectTreeState::default(),
            tab_counts: HashMap::new(),
            running_workspace_ids: HashSet::new(),
            workspace_spinner_states: HashMap::new(),
            repository_mouse_states: HashMap::new(),
            workspace_mouse_states: HashMap::new(),
            workspace_delete_mouse_states: HashMap::new(),
            repository_add_workspace_mouse_states: HashMap::new(),
            unclassified_mouse_state: Default::default(),
            add_repository_button,
        };
        panel.refresh_tree(ctx);
        ctx.subscribe_to_model(&project_organization_model, |panel, _, _, ctx| {
            panel.refresh_tree(ctx);
        });
        panel
    }

    pub fn set_tab_counts(
        &mut self,
        tab_counts: HashMap<RepositoryWorkspaceId, usize>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.tab_counts == tab_counts {
            return;
        }
        self.tab_counts = tab_counts;
        self.refresh_tree(ctx);
    }

    pub fn set_running_workspaces(
        &mut self,
        running_workspace_ids: HashSet<RepositoryWorkspaceId>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.running_workspace_ids == running_workspace_ids {
            return;
        }
        self.running_workspace_ids = running_workspace_ids;
        ctx.notify();
    }

    pub fn set_active_workspace(
        &mut self,
        workspace_id: Option<RepositoryWorkspaceId>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.state.selected_workspace_id() == workspace_id {
            return;
        }
        self.state.set_active_workspace(workspace_id);
        ctx.notify();
    }

    fn refresh_tree(&mut self, ctx: &mut ViewContext<Self>) {
        let expanded_by_repository = self
            .state
            .repositories()
            .iter()
            .map(|repository| (repository.repository_id, repository.expanded))
            .collect::<HashMap<_, _>>();
        let selected_workspace_id = self.state.selected_workspace_id();
        let repositories = self
            .project_organization_model
            .as_ref(ctx)
            .repositories()
            .cloned()
            .collect();
        let workspaces = self
            .project_organization_model
            .as_ref(ctx)
            .workspaces()
            .cloned()
            .collect();

        self.state = ProjectTreeState::from_records(repositories, workspaces, &self.tab_counts);
        for repository in &mut self.state.repositories {
            if let Some(expanded) = expanded_by_repository.get(&repository.repository_id) {
                repository.expanded = *expanded;
            }
        }
        if let Some(workspace_id) = selected_workspace_id {
            self.state.select_workspace(workspace_id);
        }

        let repository_ids = self
            .state
            .repositories()
            .iter()
            .map(|repository| repository.repository_id)
            .collect::<HashSet<_>>();
        let workspace_ids = self
            .state
            .repositories()
            .iter()
            .flat_map(|repository| repository.workspaces.iter())
            .map(|workspace| workspace.workspace_id)
            .collect::<HashSet<_>>();
        synchronize_mouse_states(&mut self.repository_mouse_states, &repository_ids);
        synchronize_mouse_states(
            &mut self.repository_add_workspace_mouse_states,
            &repository_ids,
        );
        synchronize_mouse_states(&mut self.workspace_mouse_states, &workspace_ids);
        synchronize_mouse_states(&mut self.workspace_delete_mouse_states, &workspace_ids);
        self.running_workspace_ids
            .retain(|workspace_id| workspace_ids.contains(workspace_id));
        self.workspace_spinner_states
            .retain(|workspace_id, _| workspace_ids.contains(workspace_id));
        for workspace_id in &workspace_ids {
            self.workspace_spinner_states
                .entry(*workspace_id)
                .or_default();
        }
        ctx.notify();
    }

    fn render_header(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let title = Text::new_inline(
            "Repositories",
            appearance.ui_font_family(),
            appearance.ui_font_subheading(),
        )
        .with_color(theme.main_text_color(theme.background()).into())
        .finish();

        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(title)
            .with_child(ChildView::new(&self.add_repository_button).finish())
            .finish()
    }

    fn render_repository_row(
        &self,
        repository: &RepositoryTreeNode,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let icon_color = if repository.expanded {
            theme.main_text_color(theme.background())
        } else {
            theme.sub_text_color(theme.background())
        };
        let chevron = if repository.expanded {
            icons::Icon::ChevronDown
        } else {
            icons::Icon::ChevronRight
        };
        let repository_id = repository.repository_id;
        let add_workspace_action = ProjectTreeAction::CreateWorkspace { repository_id };
        let add_workspace_tooltip = appearance
            .ui_builder()
            .tool_tip("Create workspace".to_string())
            .build()
            .finish();
        let add_workspace = icon_button(
            appearance,
            icons::Icon::Plus,
            false,
            self.repository_add_workspace_mouse_states
                .get(&repository_id)
                .expect("repository add-workspace mouse state should be initialized during tree refresh")
                .clone(),
        )
        .with_tooltip(move || add_workspace_tooltip)
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(add_workspace_action.clone());
        })
        .with_cursor(Cursor::PointingHand)
        .finish();
        let add_workspace_position_id = repository_add_workspace_position_id(repository_id);
        let add_workspace = SavePosition::new(add_workspace, &add_workspace_position_id).finish();

        let repository_icon = Container::new(
            ConstrainedBox::new(icons::Icon::Folder.to_warpui_icon(icon_color).finish())
                .with_width(16.)
                .with_height(16.)
                .finish(),
        )
        .with_uniform_padding(4.)
        .with_background(theme.surface_overlay_1())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .finish();

        let workspace_count = Text::new_inline(
            workspace_count_label(repository.workspaces.len()),
            appearance.ui_font_family(),
            appearance.ui_font_footnote(),
        )
        .with_color(theme.sub_text_color(theme.background()).into())
        .finish();

        let row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                ConstrainedBox::new(chevron.to_warpui_icon(icon_color).finish())
                    .with_width(16.)
                    .with_height(16.)
                    .finish(),
            )
            .with_child(
                Container::new(repository_icon)
                    .with_margin_left(4.)
                    .finish(),
            )
            .with_child(
                Shrinkable::new(
                    1.0,
                    Container::new(
                        Text::new_inline(
                            repository.display_name.clone(),
                            appearance.ui_font_family(),
                            appearance.ui_font_body(),
                        )
                        .with_clip(ClipConfig::ellipsis())
                        .with_color(theme.main_text_color(theme.background()).into())
                        .finish(),
                    )
                    .with_margin_left(8.)
                    .finish(),
                )
                .finish(),
            )
            .with_child(
                Container::new(workspace_count)
                    .with_margin_left(12.)
                    .with_margin_right(8.)
                    .finish(),
            )
            .with_child(add_workspace)
            .finish();
        let toggle_action = ProjectTreeAction::ToggleRepository { repository_id };

        Hoverable::new(
            self.repository_mouse_states
                .get(&repository_id)
                .expect("repository row mouse state should be initialized during tree refresh")
                .clone(),
            move |mouse_state| {
                let mut container = Container::new(row)
                    .with_horizontal_padding(8.)
                    .with_vertical_padding(6.)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
                if mouse_state.is_hovered() {
                    container = container.with_background(theme.surface_overlay_1());
                }
                container.finish()
            },
        )
        .with_cursor(Cursor::PointingHand)
        .with_defer_events_to_children()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(toggle_action.clone());
        })
        .finish()
    }

    fn render_workspace_activity_badge(
        &self,
        tab_count: usize,
        visual_state: WorkspaceVisualState,
        workspace_id: RepositoryWorkspaceId,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let metadata_color = theme.sub_text_color(theme.background());
        let running_color: pathfinder_color::ColorU = theme.terminal_colors().normal.green.into();
        let badge_background = if visual_state.should_render_running_spinner() {
            coloru_with_opacity(running_color, 14).into()
        } else {
            theme.surface_2()
        };
        let border_fill = if visual_state.should_render_running_spinner() {
            coloru_with_opacity(running_color, 42).into()
        } else {
            theme.surface_3()
        };

        let spinner = if visual_state.should_render_running_spinner() {
            let spinner_state = self
                .workspace_spinner_states
                .get(&workspace_id)
                .expect("workspace spinner state should be initialized during tree refresh")
                .clone();
            Box::new(BrailleSpinner::new(
                appearance.ui_font_family(),
                appearance.ui_font_footnote(),
                running_color,
                spinner_state,
            )) as Box<dyn Element>
        } else {
            Empty::new().finish()
        };
        let spinner_slot = ConstrainedBox::new(spinner)
            .with_width(WORKSPACE_RUNNING_INDICATOR_SLOT_WIDTH)
            .with_height(WORKSPACE_TAB_COUNT_BADGE_HEIGHT)
            .finish();

        let count = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Text::new_inline(
                    tab_count_badge_label(tab_count),
                    appearance.ui_font_family(),
                    appearance.ui_font_footnote(),
                )
                .with_color(metadata_color.into())
                .finish(),
            )
            .finish();
        let count_badge = Container::new(
            ConstrainedBox::new(count)
                .with_width(workspace_tab_count_badge_width(tab_count))
                .with_height(WORKSPACE_TAB_COUNT_BADGE_HEIGHT)
                .finish(),
        )
        .with_background(badge_background)
        .with_border(Border::all(1.).with_border_fill(border_fill))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(12.)));
        let count_badge = if visual_state.should_render_running_spinner() {
            count_badge.with_drop_shadow(
                DropShadow::new_with_standard_offset_and_spread(coloru_with_opacity(
                    running_color,
                    30,
                ))
                .with_offset(vec2f(0., 0.)),
            )
        } else {
            count_badge
        };

        ConstrainedBox::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_main_axis_alignment(MainAxisAlignment::End)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(WORKSPACE_STATUS_SLOT_GAP)
                .with_child(spinner_slot)
                .with_child(count_badge.finish())
                .finish(),
        )
        .with_width(
            WORKSPACE_RUNNING_INDICATOR_SLOT_WIDTH
                + WORKSPACE_STATUS_SLOT_GAP
                + workspace_tab_count_badge_width(tab_count),
        )
        .with_height(WORKSPACE_TAB_COUNT_BADGE_HEIGHT)
        .finish()
    }

    fn render_workspace_row(
        &self,
        workspace: &WorkspaceTreeNode,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let selected =
            workspace_row_is_selected(self.state.selected_workspace_id(), workspace.workspace_id);
        let selection_accent = theme.accent();
        let label_color = theme.main_text_color(theme.background());
        let metadata_color = theme.sub_text_color(theme.background());
        let workspace_id = workspace.workspace_id;
        let action = ProjectTreeAction::SelectWorkspace {
            workspace_id: Some(workspace_id),
        };
        let delete_action = ProjectTreeAction::DeleteWorkspace { workspace_id };
        let branch = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(3.)
            .with_child(
                ConstrainedBox::new(
                    icons::Icon::GitBranch
                        .to_warpui_icon(metadata_color)
                        .finish(),
                )
                .with_width(12.)
                .with_height(12.)
                .finish(),
            )
            .with_child(
                Shrinkable::new(
                    1.,
                    Text::new_inline(
                        workspace.branch.clone(),
                        appearance.ui_font_family(),
                        appearance.ui_font_footnote(),
                    )
                    .with_clip(ClipConfig::ellipsis())
                    .with_color(metadata_color.into())
                    .finish(),
                )
                .finish(),
            )
            .finish();
        let content = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_spacing(2.)
            .with_child(
                Text::new_inline(
                    workspace.display_name.clone(),
                    appearance.ui_font_family(),
                    appearance.ui_font_body(),
                )
                .with_clip(ClipConfig::ellipsis())
                .with_color(label_color.into())
                .finish(),
            )
            .with_child(branch)
            .finish();

        let visual_state = WorkspaceVisualState::new(
            selected,
            self.running_workspace_ids.contains(&workspace.workspace_id),
        );
        let activity_badge = self.render_workspace_activity_badge(
            workspace.tab_count,
            visual_state,
            workspace_id,
            appearance,
        );
        let selected_color: pathfinder_color::ColorU = theme.terminal_colors().normal.blue.into();
        let selected_border_color = coloru_with_opacity(selected_color, 58);
        let selected_shadow_color = coloru_with_opacity(selected_color, 34);

        let delete_tooltip = appearance
            .ui_builder()
            .tool_tip("Remove workspace".to_string())
            .build()
            .finish();
        let delete = icon_button(
            appearance,
            icons::Icon::Trash,
            false,
            self.workspace_delete_mouse_states
                .get(&workspace_id)
                .expect("workspace delete mouse state should be initialized during tree refresh")
                .clone(),
        )
        .with_tooltip(move || delete_tooltip)
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(delete_action.clone());
        })
        .with_cursor(Cursor::PointingHand)
        .finish();
        let delete_placeholder = ConstrainedBox::new(Empty::new().finish())
            .with_width(icons::ICON_DIMENSIONS)
            .with_height(icons::ICON_DIMENSIONS)
            .finish();

        Hoverable::new(
            self.workspace_mouse_states
                .get(&workspace_id)
                .expect("workspace row mouse state should be initialized during tree refresh")
                .clone(),
            move |mouse_state| {
                let delete = if should_show_workspace_delete_button(mouse_state.is_hovered()) {
                    delete
                } else {
                    delete_placeholder
                };
                let row_content = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Shrinkable::new(1.0, content).finish())
                    .with_child(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_spacing(8.)
                            .with_child(activity_badge)
                            .with_child(delete)
                            .finish(),
                    )
                    .finish();
                let mut row_container = Container::new(row_content)
                    .with_horizontal_padding(8.)
                    .with_vertical_padding(6.)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
                if selected {
                    row_container =
                        row_container.with_background(selection_accent.with_opacity(10));
                } else if mouse_state.is_hovered() {
                    row_container = row_container.with_background(theme.surface_overlay_2());
                } else {
                    row_container = row_container.with_background(theme.surface_overlay_1());
                }
                row_container = apply_workspace_selection_frame(
                    row_container,
                    visual_state,
                    selected_border_color,
                    selected_shadow_color,
                );

                let indicator = Container::new(
                    ConstrainedBox::new(Empty::new().finish())
                        .with_width(2.)
                        .finish(),
                );
                let indicator = if selected {
                    indicator
                        .with_background(selection_accent)
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(1.)))
                } else {
                    indicator
                };

                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .with_child(indicator.finish())
                    .with_child(
                        Shrinkable::new(
                            1.0,
                            Container::new(row_container.finish())
                                .with_margin_left(4.)
                                .finish(),
                        )
                        .finish(),
                    )
                    .finish()
            },
        )
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish()
    }

    fn render_unclassified_row(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let action = ProjectTreeAction::SelectWorkspace { workspace_id: None };
        let content = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                ConstrainedBox::new(
                    icons::Icon::Terminal
                        .to_warpui_icon(theme.sub_text_color(theme.background()))
                        .finish(),
                )
                .with_width(16.)
                .with_height(16.)
                .finish(),
            )
            .with_child(
                Container::new(
                    Text::new_inline(
                        "Unclassified tabs",
                        appearance.ui_font_family(),
                        appearance.ui_font_body(),
                    )
                    .with_color(theme.main_text_color(theme.background()).into())
                    .finish(),
                )
                .with_margin_left(8.)
                .finish(),
            )
            .finish();

        Hoverable::new(self.unclassified_mouse_state.clone(), move |mouse_state| {
            let mut container = Container::new(content)
                .with_horizontal_padding(8.)
                .with_vertical_padding(6.)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
            if mouse_state.is_hovered() {
                container = container.with_background(theme.surface_overlay_1());
            }
            container.finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish()
    }
}

impl Entity for ProjectTreePanel {
    type Event = ProjectTreeEvent;
}

impl TypedActionView for ProjectTreePanel {
    type Action = ProjectTreeAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            ProjectTreeAction::AddRepository => ctx.emit(ProjectTreeEvent::AddRepositoryRequested),
            ProjectTreeAction::CreateWorkspace { repository_id } => {
                ctx.emit(ProjectTreeEvent::CreateWorkspaceRequested {
                    repository_id: *repository_id,
                });
            }
            ProjectTreeAction::DeleteWorkspace { workspace_id } => {
                ctx.emit(ProjectTreeEvent::DeleteWorkspaceRequested {
                    workspace_id: *workspace_id,
                });
            }
            ProjectTreeAction::ToggleRepository { repository_id } => {
                self.state.toggle_repository(*repository_id);
                ctx.notify();
            }
            ProjectTreeAction::SelectWorkspace { workspace_id } => {
                if let Some(workspace_id) = workspace_id {
                    self.state.select_workspace(*workspace_id);
                } else {
                    self.state.selected_workspace_id = None;
                }
                ctx.emit(ProjectTreeEvent::WorkspaceSelected {
                    workspace_id: *workspace_id,
                });
                ctx.notify();
            }
        }
    }
}

impl View for ProjectTreePanel {
    fn ui_name() -> &'static str {
        "ProjectTreePanel"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let mut tree = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(8.);
        for repository in self.state.repositories() {
            let mut repository_group = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(self.render_repository_row(repository, appearance));
            if repository.expanded {
                let mut workspaces = Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .with_spacing(2.);
                for workspace in &repository.workspaces {
                    workspaces.add_child(self.render_workspace_row(workspace, appearance));
                }
                repository_group.add_child(
                    Container::new(workspaces.finish())
                        .with_margin_left(22.)
                        .with_margin_top(2.)
                        .finish(),
                );
            }
            tree.add_child(repository_group.finish());
        }

        let body: Box<dyn Element> = if self.state.repositories().is_empty() {
            Container::new(
                Text::new_inline(
                    "Add a local Git repository to create workspaces.",
                    appearance.ui_font_family(),
                    appearance.ui_font_body(),
                )
                .with_color(
                    appearance
                        .theme()
                        .sub_text_color(appearance.theme().background())
                        .into(),
                )
                .finish(),
            )
            .with_uniform_padding(8.)
            .finish()
        } else {
            Container::new(tree.finish())
                .with_horizontal_padding(4.)
                .with_vertical_padding(6.)
                .finish()
        };

        Flex::column()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(
                Container::new(self.render_header(appearance))
                    .with_horizontal_padding(12.)
                    .with_vertical_padding(10.)
                    .with_border(Border::bottom(1.).with_border_fill(theme.surface_2()))
                    .finish(),
            )
            .with_child(Shrinkable::new(1.0, body).finish())
            .with_child(
                Container::new(self.render_unclassified_row(appearance))
                    .with_uniform_padding(8.)
                    .with_border(Border::top(1.).with_border_fill(theme.surface_2()))
                    .finish(),
            )
            .finish()
    }
}

#[cfg(test)]
#[path = "project_tree_tests.rs"]
mod tests;
