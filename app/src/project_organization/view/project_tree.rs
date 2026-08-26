use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use pathfinder_color::ColorU;
use pathfinder_geometry::vector::vec2f;
use warp_core::ui::color::coloru_with_opacity;
use warp_core::ui::icons::Icon as WarpIcon;
use warp_core::ui::theme::WarpTheme;
use warpui::{
    elements::{
        Border, ChildView, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container,
        CornerRadius, CrossAxisAlignment, DropShadow, Element, Empty, Fill, Flex, Hoverable,
        MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement, Radius, SavePosition,
        ScrollbarWidth, Shrinkable, Stack, Text,
    },
    platform::Cursor,
    text_layout::ClipConfig,
    ui_components::components::UiComponent,
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

use crate::{
    appearance::Appearance,
    project_organization::{
        model::ProjectOrganizationModel,
        workspace_agent_activity::{
            workspace_activity_slot, WorkspaceActivitySlot, WorkspaceAgentActivity,
            WorkspaceAgentIdentity, WorkspaceAgentPhase,
        },
    },
    ui_components::{
        breathing_ring::{
            breathing_opacity, BreathingStateHandle, BreathingTicker, BREATHING_PERIOD,
        },
        buttons::icon_button,
        icon_with_status::{render_icon_with_status, IconWithStatusSizing, IconWithStatusVariant},
        icons,
    },
    view_components::action_button::{ActionButton, ButtonSize, SecondaryTheme},
};

use crate::project_organization::domain::{
    Repository, RepositoryId, RepositoryWorkspace, RepositoryWorkspaceId,
};

const WORKSPACE_RUNNING_DOT_SIZE: f32 = 6.;
const WORKSPACE_ACTIVITY_SLOT_SIZE: f32 = 16.;
const WORKSPACE_TREE_RAIL_WIDTH: f32 = 2.;
const WORKSPACE_GROUP_INDENT: f32 = 16.;
const REPOSITORY_GROUP_SPACING: f32 = 10.;
const WORKSPACE_AGENT_RING_WIDTH: f32 = 1.5;

const WORKSPACE_AGENT_ICON_SIZING: IconWithStatusSizing = IconWithStatusSizing {
    icon_size: 10.,
    padding: 3.,
    badge_icon_size: 8.,
    badge_padding: 1.,
    overall_size_override: Some(16.),
    badge_offset: (0., 0.),
};

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
    agent_activity: Option<WorkspaceAgentActivity>,
}

impl WorkspaceVisualState {
    pub(crate) fn new(
        is_selected: bool,
        has_running_terminal: bool,
        agent_activity: Option<WorkspaceAgentActivity>,
    ) -> Self {
        Self {
            is_selected,
            has_running_terminal,
            agent_activity,
        }
    }

    pub(crate) fn should_render_selection_frame(&self) -> bool {
        false
    }

    pub(crate) fn should_render_selection_accent(&self) -> bool {
        self.is_selected
    }

    /// 活动槽类型: agent 在场时绿点让位给头像。
    pub(crate) fn activity_slot(&self) -> WorkspaceActivitySlot {
        workspace_activity_slot(self.agent_activity, self.has_running_terminal)
    }

    pub(crate) fn should_render_running_indicator(&self) -> bool {
        matches!(self.activity_slot(), WorkspaceActivitySlot::RunningDot)
    }

    pub(crate) fn should_breathe_agent_ring(&self) -> bool {
        self.agent_activity
            .is_some_and(WorkspaceAgentActivity::should_breathe)
    }

    pub(crate) fn should_fill_idle_row(&self) -> bool {
        false
    }
}

/// 活动槽呼吸环颜色: Blocked 为黄, InProgress CLI 用 brand, Oz 用 accent。
fn agent_activity_ring_color(activity: WorkspaceAgentActivity, theme: &WarpTheme) -> ColorU {
    match activity.phase {
        WorkspaceAgentPhase::Blocked => theme.ansi_fg_yellow(),
        WorkspaceAgentPhase::InProgress => match activity.identity {
            WorkspaceAgentIdentity::Cli(agent) => agent
                .brand_color()
                .unwrap_or_else(|| theme.accent().into_solid_bias_right_color()),
            WorkspaceAgentIdentity::Oz { ambient: _ } => {
                theme.accent().into_solid_bias_right_color()
            }
        },
    }
}

fn agent_activity_icon_variant(
    identity: WorkspaceAgentIdentity,
    theme: &WarpTheme,
) -> IconWithStatusVariant {
    match identity {
        WorkspaceAgentIdentity::Oz { ambient } => IconWithStatusVariant::OzAgent {
            status: None,
            is_ambient: ambient,
        },
        WorkspaceAgentIdentity::Cli(agent) => match agent.brand_color() {
            Some(_) => IconWithStatusVariant::CLIAgent {
                agent,
                status: None,
            },
            None => IconWithStatusVariant::Neutral {
                icon: WarpIcon::Terminal,
                icon_color: theme.sub_text_color(theme.background()),
            },
        },
    }
}

fn sized_activity_slot(child: Box<dyn Element>) -> Box<dyn Element> {
    ConstrainedBox::new(
        Flex::row()
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(child)
            .finish(),
    )
    .with_width(WORKSPACE_ACTIVITY_SLOT_SIZE)
    .with_height(WORKSPACE_ACTIVITY_SLOT_SIZE)
    .finish()
}

/// 显示名与真实分支相同时不再重复第二行; 分支信息仍由显示名本身承担。
fn workspace_shows_branch_subtitle(display_name: &str, branch: &str) -> bool {
    display_name != branch
}

fn workspace_count_pill_label(workspace_count: usize) -> String {
    workspace_count.to_string()
}

fn tab_count_badge_label(tab_count: usize) -> String {
    if tab_count > 99 {
        "99+".to_string()
    } else {
        tab_count.to_string()
    }
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
    clipped_scroll_state: ClippedScrollStateHandle,
    tab_counts: HashMap<RepositoryWorkspaceId, usize>,
    running_workspace_ids: HashSet<RepositoryWorkspaceId>,
    agent_activities: HashMap<RepositoryWorkspaceId, WorkspaceAgentActivity>,
    workspace_breathing_states: HashMap<RepositoryWorkspaceId, BreathingStateHandle>,
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
            clipped_scroll_state: Default::default(),
            tab_counts: HashMap::new(),
            running_workspace_ids: HashSet::new(),
            agent_activities: HashMap::new(),
            workspace_breathing_states: HashMap::new(),
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

    /// 设置各 workspace 的 agent 活动,并同步呼吸环状态。
    pub fn set_agent_activities(
        &mut self,
        agent_activities: HashMap<RepositoryWorkspaceId, WorkspaceAgentActivity>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.agent_activities == agent_activities {
            return;
        }
        self.agent_activities = agent_activities;
        self.sync_breathing_states();
        ctx.notify();
    }

    fn sync_breathing_states(&mut self) {
        self.workspace_breathing_states.retain(|id, _| {
            self.agent_activities
                .get(id)
                .is_some_and(|activity| activity.should_breathe())
        });
        for (id, activity) in &self.agent_activities {
            if activity.should_breathe() {
                self.workspace_breathing_states.entry(*id).or_default();
            }
        }
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
        self.agent_activities
            .retain(|id, _| workspace_ids.contains(id));
        self.sync_breathing_states();
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

        let workspace_count = Container::new(
            Text::new_inline(
                workspace_count_pill_label(repository.workspaces.len()),
                appearance.ui_font_family(),
                appearance.ui_font_footnote(),
            )
            .with_color(theme.sub_text_color(theme.background()).into())
            .finish(),
        )
        .with_horizontal_padding(5.)
        .with_vertical_padding(1.)
        .with_background(theme.surface_overlay_1())
        .with_border(Border::all(1.).with_border_fill(theme.surface_2()))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
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
                    .with_margin_left(6.)
                    .finish(),
                )
                .finish(),
            )
            .with_child(
                Container::new(workspace_count)
                    .with_margin_left(8.)
                    .with_margin_right(6.)
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

    fn render_workspace_running_dot(
        visual_state: WorkspaceVisualState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let running_color: pathfinder_color::ColorU = theme.terminal_colors().normal.green.into();
        let dot = if visual_state.should_render_running_indicator() {
            Container::new(Empty::new().finish())
                .with_background(running_color)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                    WORKSPACE_RUNNING_DOT_SIZE / 2.,
                )))
                .with_drop_shadow(
                    DropShadow::new_with_standard_offset_and_spread(coloru_with_opacity(
                        running_color,
                        48,
                    ))
                    .with_offset(vec2f(0., 0.)),
                )
                .finish()
        } else {
            Empty::new().finish()
        };
        ConstrainedBox::new(
            Flex::row()
                .with_main_axis_alignment(MainAxisAlignment::Center)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    ConstrainedBox::new(dot)
                        .with_width(WORKSPACE_RUNNING_DOT_SIZE)
                        .with_height(WORKSPACE_RUNNING_DOT_SIZE)
                        .finish(),
                )
                .finish(),
        )
        .with_width(WORKSPACE_RUNNING_DOT_SIZE)
        .with_height(WORKSPACE_RUNNING_DOT_SIZE)
        .finish()
    }

    fn render_workspace_activity_slot(
        &self,
        workspace_id: RepositoryWorkspaceId,
        visual_state: WorkspaceVisualState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        match visual_state.activity_slot() {
            WorkspaceActivitySlot::Empty => sized_activity_slot(Empty::new().finish()),
            WorkspaceActivitySlot::RunningDot => {
                sized_activity_slot(Self::render_workspace_running_dot(visual_state, appearance))
            }
            WorkspaceActivitySlot::Agent(activity) => {
                self.render_workspace_agent_avatar(workspace_id, activity, appearance)
            }
        }
    }

    fn render_workspace_agent_avatar(
        &self,
        workspace_id: RepositoryWorkspaceId,
        activity: WorkspaceAgentActivity,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let avatar = render_icon_with_status(
            agent_activity_icon_variant(activity.identity, theme),
            &WORKSPACE_AGENT_ICON_SIZING,
            theme,
            theme.background(),
        );
        let ring_color = agent_activity_ring_color(activity, theme);
        let (opacity, ticker) = match activity.phase {
            WorkspaceAgentPhase::InProgress => {
                let handle = self
                    .workspace_breathing_states
                    .get(&workspace_id)
                    .expect("InProgress agent 必须先由 sync_breathing_states 插入呼吸环 handle");
                (
                    breathing_opacity(handle.elapsed(), BREATHING_PERIOD),
                    Some(BreathingTicker::new(handle.clone())),
                )
            }
            WorkspaceAgentPhase::Blocked => (255, None),
        };
        // breathing_opacity 返回 0-255 alpha; coloru_with_opacity 按 0-100 百分比缩放,不能直接套用。
        let ringed_avatar = Container::new(avatar)
            .with_border(
                Border::all(WORKSPACE_AGENT_RING_WIDTH).with_border_fill(ColorU::new(
                    ring_color.r,
                    ring_color.g,
                    ring_color.b,
                    opacity,
                )),
            )
            .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
            .finish();
        let content = match ticker {
            Some(ticker) => Stack::new()
                .with_child(ringed_avatar)
                .with_child(Box::new(ticker))
                .finish(),
            None => ringed_avatar,
        };
        sized_activity_slot(content)
    }

    fn render_workspace_tab_count(
        tab_count: usize,
        visual_state: WorkspaceVisualState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let color = if visual_state.should_render_selection_accent() {
            theme.accent().into_solid_bias_right_color()
        } else {
            theme
                .sub_text_color(theme.background())
                .into_solid_bias_right_color()
        };
        ConstrainedBox::new(
            Text::new_inline(
                tab_count_badge_label(tab_count),
                appearance.ui_font_family(),
                appearance.ui_font_footnote(),
            )
            .with_color(color.into())
            .finish(),
        )
        .with_min_width(14.)
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
        let selection_accent_color = selection_accent.into_solid_bias_right_color();
        let label_color = if selected {
            selection_accent_color
        } else {
            theme
                .main_text_color(theme.background())
                .into_solid_bias_right_color()
        };
        let metadata_color = theme.sub_text_color(theme.background());
        let workspace_id = workspace.workspace_id;
        let action = ProjectTreeAction::SelectWorkspace {
            workspace_id: Some(workspace_id),
        };
        let delete_action = ProjectTreeAction::DeleteWorkspace { workspace_id };
        let name = Text::new_inline(
            workspace.display_name.clone(),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_clip(ClipConfig::ellipsis())
        .with_color(label_color.into())
        .finish();
        let content = if workspace_shows_branch_subtitle(&workspace.display_name, &workspace.branch)
        {
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
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_spacing(1.)
                .with_child(name)
                .with_child(branch)
                .finish()
        } else {
            name
        };

        let visual_state = WorkspaceVisualState::new(
            selected,
            self.running_workspace_ids.contains(&workspace.workspace_id),
            self.agent_activities.get(&workspace.workspace_id).copied(),
        );
        let activity_slot =
            self.render_workspace_activity_slot(workspace.workspace_id, visual_state, appearance);
        let tab_count =
            Self::render_workspace_tab_count(workspace.tab_count, visual_state, appearance);
        let labeled_content = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(8.)
            .with_child(activity_slot)
            .with_child(Shrinkable::new(1.0, content).finish())
            .finish();

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
                    .with_child(Shrinkable::new(1.0, labeled_content).finish())
                    .with_child(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_spacing(8.)
                            .with_child(tab_count)
                            .with_child(delete)
                            .finish(),
                    )
                    .finish();
                let mut row_container = Container::new(row_content)
                    .with_horizontal_padding(8.)
                    .with_vertical_padding(5.)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(5.)));
                if visual_state.should_render_selection_accent() {
                    row_container =
                        row_container.with_background(selection_accent.with_opacity(10));
                } else if mouse_state.is_hovered() {
                    row_container = row_container.with_background(theme.surface_overlay_2());
                } else if visual_state.should_fill_idle_row() {
                    row_container = row_container.with_background(theme.surface_overlay_1());
                }
                if visual_state.should_render_selection_frame() {
                    row_container = row_container
                        .with_border(Border::all(1.).with_border_fill(selection_accent));
                }

                let rail = Container::new(
                    ConstrainedBox::new(Empty::new().finish())
                        .with_width(WORKSPACE_TREE_RAIL_WIDTH)
                        .finish(),
                );
                let rail = if visual_state.should_render_selection_accent() {
                    rail.with_background(selection_accent)
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(1.)))
                        .with_drop_shadow(
                            DropShadow::new_with_standard_offset_and_spread(coloru_with_opacity(
                                selection_accent_color,
                                48,
                            ))
                            .with_offset(vec2f(0., 0.)),
                        )
                } else {
                    rail.with_background(theme.surface_overlay_2())
                };

                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .with_child(rail.finish())
                    .with_child(
                        Shrinkable::new(
                            1.0,
                            Container::new(row_container.finish())
                                .with_margin_left(6.)
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
            .with_spacing(REPOSITORY_GROUP_SPACING);
        for repository in self.state.repositories() {
            let mut repository_group = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(self.render_repository_row(repository, appearance));
            if repository.expanded && !repository.workspaces.is_empty() {
                let mut workspaces = Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .with_spacing(0.);
                for workspace in &repository.workspaces {
                    workspaces.add_child(self.render_workspace_row(workspace, appearance));
                }
                repository_group.add_child(
                    Container::new(workspaces.finish())
                        .with_margin_left(WORKSPACE_GROUP_INDENT)
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
        let scrollable_body = ClippedScrollable::vertical(
            self.clipped_scroll_state.clone(),
            body,
            ScrollbarWidth::Auto,
            theme.disabled_text_color(theme.background()).into(),
            theme.main_text_color(theme.background()).into(),
            Fill::None,
        )
        .finish();

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
            .with_child(Shrinkable::new(1.0, scrollable_body).finish())
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
