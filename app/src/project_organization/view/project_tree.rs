use std::{
    any::Any,
    collections::{HashMap, HashSet},
    hash::Hash,
};

use pathfinder_color::ColorU;
use pathfinder_geometry::vector::vec2f;
use warp_core::ui::color::coloru_with_opacity;
use warp_core::ui::icons::Icon as WarpIcon;
use warp_core::ui::theme::{Fill as ThemeFill, WarpTheme};
use warpui::{
    assets::asset_cache::AssetSource,
    elements::{
        AcceptedByDropTarget, Border, CacheOption, ChildAnchor, ChildView, ClippedScrollStateHandle,
        ClippedScrollable, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, DragAxis,
        Draggable, DraggableState, DropShadow, DropTarget, DropTargetData, Element, Empty, Fill,
        Flex, Hoverable, Image, MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentAnchor,
        ParentElement, Radius, SavePosition, ScrollTarget, ScrollToPositionMode, ScrollbarWidth,
        Shrinkable, Text,
    },
    platform::Cursor,
    text_layout::ClipConfig,
    ui_components::components::UiComponent,
    AppContext, Entity, EventContext, ModelHandle, SingletonEntity, TypedActionView, View,
    ViewContext, ViewHandle,
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
        buttons::icon_button,
        icon_with_status::render_cli_agent_logo,
        icons,
        relative_time_label::RelativeTimeLabel,
        spinner::{BrailleSpinner, SpinnerStateHandle},
    },
    view_components::action_button::{ActionButton, ButtonSize, SecondaryTheme},
};

use crate::project_organization::domain::{
    Repository, RepositoryId, RepositoryWorkspace, RepositoryWorkspaceId,
};
use crate::project_organization::project_tree_tab::{
    workspace_parent_activity_slot, ProjectTreeTabId, ProjectTreeTabNode, TabNodeActivity,
};
use crate::project_organization::view::demoting_done_status_glyph::DemotingDoneStatusGlyph;

const WORKSPACE_RUNNING_DOT_SIZE: f32 = 6.;
const WORKSPACE_TREE_RAIL_WIDTH: f32 = 2.;
const WORKSPACE_GROUP_INDENT: f32 = 16.;
const REPOSITORY_GROUP_SPACING: f32 = 10.;
const TREE_CHEVRON_SIZE: f32 = 16.;
const TREE_ROW_ICON_SIZE: f32 = 16.;
const TREE_ICON_GAP: f32 = 6.;
pub(crate) const TREE_TRAILING_ACTION_SPACING: f32 = 8.;
/// Shared right-side meta column for workspace tab-count and agent relative time.
/// Wide enough for `99+` / `59m` / `23h` / `now` at footnote size; text is end-aligned.
pub(crate) const TREE_TRAILING_META_WIDTH: f32 = 32.;
/// Workspace rows reserve two hover actions (new tab + delete). Tab rows use the same
/// footprint (close end-aligned) so meta columns share one vertical edge.
const TREE_TRAILING_ACTION_SLOTS: f32 = 2.;
/// 页签图标与 workspace 分支图标对齐: 与 chevron 同宽,行内 Flex spacing 再补 gap。
const TAB_UNDER_WORKSPACE_INDENT: f32 = TREE_CHEVRON_SIZE;
const ITERM_PROMPT_ICON_PATH: &str = "bundled/svg/iterm-prompt.svg";
/// Orca CompactAgentRow: [状态点][品牌标]
const TAB_AGENT_STATUS_SLOT_SIZE: f32 = 12.;
const TAB_AGENT_BRAND_ICON_SIZE: f32 = 14.;
const TAB_AGENT_LEADING_GAP: f32 = 4.;
/// Orca model chip max width (preview / CompactAgentRow).
const MODEL_CHIP_MAX_WIDTH: f32 = 72.;

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
    pub expanded: bool,
    pub tabs: Vec<ProjectTreeTabNode>,
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
    Tab {
        workspace_id: RepositoryWorkspaceId,
        tab_id: ProjectTreeTabId,
    },
}

#[derive(Clone, Copy, Debug)]
struct RepositoryDropTarget {
    repository_id: RepositoryId,
}

impl DropTargetData for RepositoryDropTarget {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) fn repository_drop_indices(
    ordered_ids: &[RepositoryId],
    source_id: RepositoryId,
    target_id: RepositoryId,
) -> Option<(usize, usize)> {
    let from = ordered_ids.iter().position(|id| *id == source_id)?;
    let to = ordered_ids.iter().position(|id| *id == target_id)?;
    (from != to).then_some((from, to))
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
        tabs: &HashMap<RepositoryWorkspaceId, Vec<ProjectTreeTabNode>>,
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
            left.sort_index
                .cmp(&right.sort_index)
                .then_with(|| left.created_at.cmp(&right.created_at))
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
                        expanded: workspace.expanded,
                        tabs: tabs.get(&workspace.id).cloned().unwrap_or_default(),
                    })
                    .collect();
                RepositoryTreeNode {
                    repository_id: repository.id,
                    display_name: repository.display_name,
                    expanded: repository.expanded,
                    workspaces,
                }
            })
            .collect::<Vec<_>>();
        Self::new(repositories)
    }

    pub fn visible_rows(&self) -> Vec<ProjectTreeRow> {
        let mut rows = Vec::new();
        for repository in &self.repositories {
            rows.push(ProjectTreeRow::Repository(repository.repository_id));
            if !repository.expanded {
                continue;
            }
            for workspace in &repository.workspaces {
                rows.push(ProjectTreeRow::Workspace(workspace.workspace_id));
                if !workspace.expanded {
                    continue;
                }
                for tab in &workspace.tabs {
                    rows.push(ProjectTreeRow::Tab {
                        workspace_id: workspace.workspace_id,
                        tab_id: tab.id,
                    });
                }
            }
        }
        rows
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
        self.select_workspace_internal(workspace_id, true)
    }

    /// 点名称: 未选中或已收起则选中并展开; 已选中且已展开则收起。
    pub fn select_or_toggle_workspace(&mut self, workspace_id: RepositoryWorkspaceId) -> bool {
        let already_selected = self.selected_workspace_id == Some(workspace_id);
        let expanded = self.workspace_is_expanded(workspace_id);
        if already_selected && expanded {
            self.toggle_workspace_expanded(workspace_id)
        } else {
            self.select_workspace(workspace_id)
        }
    }

    fn workspace_is_expanded(&self, workspace_id: RepositoryWorkspaceId) -> bool {
        self.workspace_expanded(workspace_id).unwrap_or(false)
    }

    fn workspace_expanded(&self, workspace_id: RepositoryWorkspaceId) -> Option<bool> {
        self.repositories.iter().find_map(|repository| {
            repository.workspaces.iter().find_map(|workspace| {
                (workspace.workspace_id == workspace_id).then_some(workspace.expanded)
            })
        })
    }

    fn repository_expanded(&self, repository_id: RepositoryId) -> Option<bool> {
        self.repositories
            .iter()
            .find(|repository| repository.repository_id == repository_id)
            .map(|repository| repository.expanded)
    }

    fn repository_id_for_workspace(
        &self,
        workspace_id: RepositoryWorkspaceId,
    ) -> Option<RepositoryId> {
        self.repositories.iter().find_map(|repository| {
            repository
                .workspaces
                .iter()
                .any(|workspace| workspace.workspace_id == workspace_id)
                .then_some(repository.repository_id)
        })
    }

    fn select_workspace_internal(
        &mut self,
        workspace_id: RepositoryWorkspaceId,
        expand: bool,
    ) -> bool {
        let exists = self.repositories.iter().any(|repository| {
            repository
                .workspaces
                .iter()
                .any(|workspace| workspace.workspace_id == workspace_id)
        });
        if !exists {
            return false;
        }
        self.selected_workspace_id = Some(workspace_id);
        if expand {
            if let Some(workspace) = self.workspace_mut(workspace_id) {
                workspace.expanded = true;
            }
        }
        true
    }

    pub fn toggle_workspace_expanded(&mut self, workspace_id: RepositoryWorkspaceId) -> bool {
        let Some(workspace) = self.workspace_mut(workspace_id) else {
            return false;
        };
        workspace.expanded = !workspace.expanded;
        true
    }

    fn workspace_mut(
        &mut self,
        workspace_id: RepositoryWorkspaceId,
    ) -> Option<&mut WorkspaceTreeNode> {
        self.repositories.iter_mut().find_map(|repository| {
            repository
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.workspace_id == workspace_id)
        })
    }

    pub fn set_active_workspace(&mut self, workspace_id: Option<RepositoryWorkspaceId>) {
        if let Some(workspace_id) = workspace_id {
            if self.select_workspace_internal(workspace_id, false) {
                return;
            }
        }
        self.selected_workspace_id = None;
    }

    pub fn selected_workspace_id(&self) -> Option<RepositoryWorkspaceId> {
        self.selected_workspace_id
    }

    pub fn reveal_workspace(&mut self, workspace_id: RepositoryWorkspaceId) -> bool {
        let Some(repository) = self.repositories.iter_mut().find(|repository| {
            repository
                .workspaces
                .iter()
                .any(|workspace| workspace.workspace_id == workspace_id)
        }) else {
            return false;
        };
        repository.expanded = true;
        let Some(workspace) = repository
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_id == workspace_id)
        else {
            return false;
        };
        workspace.expanded = true;
        self.selected_workspace_id = Some(workspace_id);
        true
    }
}

#[derive(Clone, Debug)]
pub enum ProjectTreeAction {
    AddRepository,
    CreateWorkspace {
        repository_id: RepositoryId,
    },
    DeleteRepository {
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
    ToggleWorkspace {
        workspace_id: RepositoryWorkspaceId,
    },
    SelectTab {
        workspace_id: RepositoryWorkspaceId,
        tab_id: ProjectTreeTabId,
    },
    CloseTab {
        workspace_id: RepositoryWorkspaceId,
        tab_id: ProjectTreeTabId,
    },
    NewTab {
        workspace_id: RepositoryWorkspaceId,
    },
    ReorderTabs {
        workspace_id: RepositoryWorkspaceId,
        from: usize,
        to: usize,
    },
    RepositoryDragStarted {
        source_id: RepositoryId,
        from: usize,
        centers: Vec<f32>,
    },
    RepositoryDragMoved {
        pointer_y: f32,
    },
    RepositoryDragDropped {
        pointer_y: f32,
    },
}

#[derive(Clone, Debug)]
pub enum ProjectTreeEvent {
    AddRepositoryRequested,
    CreateWorkspaceRequested {
        repository_id: RepositoryId,
    },
    DeleteRepositoryRequested {
        repository_id: RepositoryId,
    },
    DeleteWorkspaceRequested {
        workspace_id: RepositoryWorkspaceId,
    },
    WorkspaceSelected {
        workspace_id: Option<RepositoryWorkspaceId>,
    },
    TabSelected {
        workspace_id: RepositoryWorkspaceId,
        tab_id: ProjectTreeTabId,
    },
    TabCloseRequested {
        workspace_id: RepositoryWorkspaceId,
        tab_id: ProjectTreeTabId,
    },
    NewTabRequested {
        workspace_id: RepositoryWorkspaceId,
    },
    TabsReordered {
        workspace_id: RepositoryWorkspaceId,
        from: usize,
        to: usize,
    },
    RepositoriesReordered {
        from: usize,
        to: usize,
    },
    RepositoryExpandedChanged {
        repository_id: RepositoryId,
        expanded: bool,
    },
    WorkspaceExpandedChanged {
        workspace_id: RepositoryWorkspaceId,
        expanded: bool,
    },
}

fn repository_add_workspace_position_id(repository_id: RepositoryId) -> String {
    format!("project_tree:repository:{repository_id}:add_workspace")
}

fn repository_remove_position_id(repository_id: RepositoryId) -> String {
    format!("project_tree:repository:{repository_id}:remove")
}

fn should_show_workspace_hover_actions(workspace_row_hovered: bool) -> bool {
    workspace_row_hovered
}

fn should_show_repository_hover_actions(repository_row_hovered: bool) -> bool {
    repository_row_hovered
}

pub(crate) fn repository_block_position_id(repository_id: RepositoryId) -> String {
    format!("project_tree:repository:{repository_id}:block")
}

pub(crate) fn workspace_row_position_id(workspace_id: RepositoryWorkspaceId) -> String {
    format!("project_tree:workspace:{workspace_id}:row")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CurrentWorkspaceLocation {
    pub repository_id: RepositoryId,
    pub repository_name: String,
    pub workspace_id: RepositoryWorkspaceId,
    pub workspace_name: String,
}

pub(crate) fn current_workspace_location(
    repositories: &[RepositoryTreeNode],
    selected_workspace_id: Option<RepositoryWorkspaceId>,
) -> Option<CurrentWorkspaceLocation> {
    let selected_workspace_id = selected_workspace_id?;
    for repository in repositories {
        if let Some(workspace) = repository
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == selected_workspace_id)
        {
            return Some(CurrentWorkspaceLocation {
                repository_id: repository.repository_id,
                repository_name: repository.display_name.clone(),
                workspace_id: workspace.workspace_id,
                workspace_name: workspace.display_name.clone(),
            });
        }
    }
    None
}

pub(crate) fn repository_insert_index_for_centers(centers: &[f32], pointer_y: f32) -> usize {
    if centers.is_empty() {
        return 0;
    }
    for (index, center) in centers.iter().enumerate() {
        if pointer_y < *center {
            return index;
        }
    }
    centers.len() - 1
}

pub(crate) fn repository_drag_display_order(
    ordered_ids: &[RepositoryId],
    from: usize,
    to: usize,
) -> Vec<RepositoryId> {
    let mut ids = ordered_ids.to_vec();
    if from == to || from >= ids.len() || to >= ids.len() {
        return ids;
    }
    let item = ids.remove(from);
    ids.insert(to, item);
    ids
}

fn repository_block_centers(ctx: &EventContext, ordered_ids: &[RepositoryId]) -> Option<Vec<f32>> {
    ordered_ids
        .iter()
        .map(|id| {
            ctx.element_position_by_id(repository_block_position_id(*id))
                .map(|bounds| bounds.center().y())
        })
        .collect()
}

#[derive(Clone, Debug)]
struct RepositoryDragPreview {
    source_id: RepositoryId,
    from: usize,
    to: usize,
    centers: Vec<f32>,
}

fn workspace_row_is_selected(
    selected_workspace_id: Option<RepositoryWorkspaceId>,
    workspace_id: RepositoryWorkspaceId,
) -> bool {
    selected_workspace_id == Some(workspace_id)
}

/// Depth Stack: selected workspace wraps head + child tabs as one elevated group.
fn workspace_group_is_selected(
    selected_workspace_id: Option<RepositoryWorkspaceId>,
    workspace_id: RepositoryWorkspaceId,
) -> bool {
    workspace_row_is_selected(selected_workspace_id, workspace_id)
}

/// Legacy per-row accent fill. Depth Stack never paints accent on the workspace head —
/// selection chrome lives on the elevated group wrapper (and active-tab inset).
fn workspace_row_shows_selection_accent(_workspace: &WorkspaceTreeNode, _is_selected: bool) -> bool {
    false
}

// Depth Stack tokens (group from 2026-10-20 aesthetic; active tab = S2 fake inset slot).
const DEPTH_STACK_GROUP_BG: ColorU = ColorU {
    r: 34,
    g: 34,
    b: 40,
    a: 255,
}; // #222228 elevated solid (mock gradient midpoint)
const DEPTH_STACK_GROUP_HAIRLINE: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 26,
}; // rgba(255,255,255,.1)
const DEPTH_STACK_GROUP_RADIUS: f32 = 14.;
const DEPTH_STACK_GROUP_PAD_Y: f32 = 5.;
/// S2 fake inset slot — outer dark lip ring (1px) + inner deeper fill (no DropShadow inset).
/// Outer lip sits darker than the slot face so the tab reads pressed into Depth Stack.
const DEPTH_STACK_INSET_LIP_BG: ColorU = ColorU {
    r: 8,
    g: 8,
    b: 10,
    a: 255,
}; // #08080a dark slot lip
const DEPTH_STACK_INSET_LIP_PAD: f32 = 1.;
const DEPTH_STACK_INSET_BG: ColorU = ColorU {
    r: 20,
    g: 20,
    b: 24,
    a: 255,
}; // #141418 (S2 slot face — deeper than prior #1a1a1f)
const DEPTH_STACK_INSET_RADIUS: f32 = 9.;
const DEPTH_STACK_INSET_INNER_RADIUS: f32 = 8.;
const DEPTH_STACK_INSET_BORDER: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 140,
}; // rgba(0,0,0,.55) — S2 slot lip edge
/// Shared horizontal inset inside Depth Stack: workspace head pad + tab slot/wash gutter.
/// Slot outer edge lines up with the head row's content box (render_workspace_row H pad).
const DEPTH_STACK_ROW_INSET_H: f32 = 8.;
const DEPTH_STACK_TAB_GUTTER_H: f32 = DEPTH_STACK_ROW_INSET_H;
const DEPTH_STACK_TAB_GUTTER_V: f32 = 1.;
/// Inactive hover: light white wash — never a solid gray pill like the old raised chip.
const DEPTH_STACK_TAB_HOVER_WASH: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 8,
}; // ~3% white (v4 `.d3 .tab:hover`)
const DEPTH_STACK_TAB_HOVER_RADIUS: f32 = 8.;
const DEPTH_STACK_HEAD_LABEL: ColorU = ColorU {
    r: 244,
    g: 244,
    b: 248,
    a: 255,
}; // #f4f4f8
const DEPTH_STACK_ACTIVE_TAB_LABEL: ColorU = ColorU {
    r: 246,
    g: 246,
    b: 248,
    a: 255,
}; // #f6f6f8
const DEPTH_STACK_INACTIVE_TAB_LABEL: ColorU = ColorU {
    r: 120,
    g: 120,
    b: 128,
    a: 255,
}; // #787880
/// Model accessory on the right — secondary type, never a gray pill.
const DEPTH_STACK_MODEL_LABEL: ColorU = ColorU {
    r: 154,
    g: 154,
    b: 164,
    a: 255,
}; // #9a9aa4

/// Static fill policy for Depth Stack tabs (hover is render-time; inset must not match wash).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DepthStackTabFill {
    /// S2 fake inset slot: outer dark lip + inner deeper fill (no outer / inset DropShadow).
    ActiveInset,
    /// Faint white wash, no border / no shadow.
    HoverWash,
    None,
}

/// Active always wins: hovering the active tab keeps the inset (no extra wash).
fn depth_stack_tab_fill(is_active: bool, is_hovered: bool) -> DepthStackTabFill {
    if is_active {
        DepthStackTabFill::ActiveInset
    } else if is_hovered {
        DepthStackTabFill::HoverWash
    } else {
        DepthStackTabFill::None
    }
}

fn depth_stack_group_drop_shadow() -> DropShadow {
    // Mock: 0 10px 28px rgba(0,0,0,.55) — single shadow (no multi-layer / inset).
    DropShadow {
        color: ColorU::new(0, 0, 0, 140),
        offset: vec2f(0., 10.),
        blur_radius: 28.,
        spread_radius: 0.,
    }
}

/// S2 fake inset slot: outer 1px dark lip + inner `#141418` face (WarpUI has no inset shadow).
/// H gutter = DEPTH_STACK_ROW_INSET_H so slot outer edges align with workspace-head content.
fn depth_stack_active_inset_slot(child: Box<dyn Element>) -> Box<dyn Element> {
    let inner = Container::new(child)
        .with_horizontal_padding(DEPTH_STACK_ROW_INSET_H)
        .with_vertical_padding(5.)
        .with_background(DEPTH_STACK_INSET_BG)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
            DEPTH_STACK_INSET_INNER_RADIUS,
        )))
        .finish();
    Container::new(inner)
        .with_horizontal_padding(DEPTH_STACK_INSET_LIP_PAD)
        .with_vertical_padding(DEPTH_STACK_INSET_LIP_PAD)
        .with_background(DEPTH_STACK_INSET_LIP_BG)
        .with_border(Border::all(1.).with_border_fill(DEPTH_STACK_INSET_BORDER))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(DEPTH_STACK_INSET_RADIUS)))
        .with_horizontal_margin(DEPTH_STACK_TAB_GUTTER_H)
        .with_margin_top(DEPTH_STACK_TAB_GUTTER_V)
        .with_margin_bottom(DEPTH_STACK_TAB_GUTTER_V)
        .finish()
}

/// Elevated Depth Stack chrome wrapping the selected workspace head + tabs.
fn depth_stack_selected_group(child: Box<dyn Element>) -> Box<dyn Element> {
    Container::new(child)
        .with_background(DEPTH_STACK_GROUP_BG)
        .with_border(Border::all(1.).with_border_fill(DEPTH_STACK_GROUP_HAIRLINE))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(DEPTH_STACK_GROUP_RADIUS)))
        .with_vertical_padding(DEPTH_STACK_GROUP_PAD_Y)
        .with_drop_shadow(depth_stack_group_drop_shadow())
        .with_margin_bottom(4.)
        .finish()
}

fn tree_row_icon(
    icon: icons::Icon,
    color: warp_core::ui::theme::Fill,
    size: f32,
) -> Box<dyn Element> {
    ConstrainedBox::new(icon.to_warpui_icon(color).finish())
        .with_width(size)
        .with_height(size)
        .finish()
}

fn tree_status_icon_offset() -> f32 {
    TREE_CHEVRON_SIZE + TREE_ICON_GAP
}

fn tab_status_icon_offset() -> f32 {
    TAB_UNDER_WORKSPACE_INDENT + TREE_ICON_GAP
}

fn tree_name_offset() -> f32 {
    tree_status_icon_offset() + TREE_ROW_ICON_SIZE + TREE_ICON_GAP
}

fn tab_name_offset() -> f32 {
    tab_status_icon_offset() + TREE_ROW_ICON_SIZE + TREE_ICON_GAP
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

    /// Depth Stack: no per-row accent wash/frame. Group elevated chrome + active-tab chip instead.
    pub(crate) fn should_render_selection_accent(&self) -> bool {
        false
    }

    /// Active tab inside Depth Stack uses an elevated gray chip (not accent fill).
    pub(crate) fn should_render_active_tab_chip(&self) -> bool {
        self.is_selected
    }

    /// Selected workspace head / meta: brighter label (group already elevated).
    pub(crate) fn should_emphasize_workspace_label(&self) -> bool {
        self.is_selected
    }

    /// 活动槽类型: agent 在场时绿点让位给头像。
    pub(crate) fn activity_slot(&self) -> WorkspaceActivitySlot {
        workspace_activity_slot(self.agent_activity.clone(), self.has_running_terminal)
    }

    pub(crate) fn should_render_running_indicator(&self) -> bool {
        matches!(self.activity_slot(), WorkspaceActivitySlot::RunningDot)
    }

    pub(crate) fn needs_agent_status_spinner(&self) -> bool {
        self.agent_activity
            .as_ref()
            .is_some_and(WorkspaceAgentActivity::needs_status_spinner)
    }

    pub(crate) fn should_fill_idle_row(&self) -> bool {
        false
    }
}

fn agent_status_dot_color(phase: WorkspaceAgentPhase, theme: &WarpTheme) -> ColorU {
    match phase {
        WorkspaceAgentPhase::Idle => ColorU::new(163, 163, 163, 102),
        WorkspaceAgentPhase::InProgress => theme.ansi_fg_yellow(),
        WorkspaceAgentPhase::Blocked | WorkspaceAgentPhase::Failed => theme.ansi_fg_red(),
        WorkspaceAgentPhase::Waiting => theme.ansi_fg_yellow(),
        WorkspaceAgentPhase::Done => theme.ansi_fg_green(),
    }
}

fn render_agent_brand_icon(identity: WorkspaceAgentIdentity, theme: &WarpTheme) -> Box<dyn Element> {
    let sub_text = theme.sub_text_color(theme.background());
    let icon = match identity {
        WorkspaceAgentIdentity::Oz { ambient: true } => WarpIcon::OzCloud
            .to_warpui_icon(theme.main_text_color(theme.background()))
            .finish(),
        WorkspaceAgentIdentity::Oz { ambient: false } => WarpIcon::Oz
            .to_warpui_icon(theme.main_text_color(theme.background()))
            .finish(),
        WorkspaceAgentIdentity::Cli(agent) => {
            render_cli_agent_logo(agent, theme.main_text_color(theme.background()), sub_text)
        }
    };
    ConstrainedBox::new(icon)
        .with_width(TAB_AGENT_BRAND_ICON_SIZE)
        .with_height(TAB_AGENT_BRAND_ICON_SIZE)
        .finish()
}

/// 固定 16×16 槽,把绿点 / 终端图标 / agent 头像都居中在和 workspace 分支图标同一列。
/// ConstrainedBox 只收紧子约束、不强制返回尺寸,所以用 Flex Max 把槽撑满。
fn tree_status_slot(child: Box<dyn Element>) -> Box<dyn Element> {
    ConstrainedBox::new(
        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(child)
            .finish(),
    )
    .with_width(TREE_ROW_ICON_SIZE)
    .with_height(TREE_ROW_ICON_SIZE)
    .finish()
}

fn sized_activity_slot(child: Box<dyn Element>) -> Box<dyn Element> {
    tree_status_slot(child)
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

/// Width of the shared trailing action strip (two icon slots + spacing).
pub(crate) fn tree_trailing_actions_width() -> f32 {
    TREE_TRAILING_ACTION_SLOTS * icons::ICON_DIMENSIONS
        + (TREE_TRAILING_ACTION_SLOTS - 1.) * TREE_TRAILING_ACTION_SPACING
}

/// Right-align `child` inside the shared meta column so tab-count and relative time share an edge.
fn render_tree_trailing_meta(child: Box<dyn Element>) -> Box<dyn Element> {
    ConstrainedBox::new(
        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::End)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(child)
            .finish(),
    )
    .with_width(TREE_TRAILING_META_WIDTH)
    .finish()
}

fn render_tree_trailing_actions(actions: Box<dyn Element>) -> Box<dyn Element> {
    ConstrainedBox::new(actions)
        .with_width(tree_trailing_actions_width())
        .finish()
}

fn synchronize_mouse_states<Id, State>(mouse_states: &mut HashMap<Id, State>, ids: &HashSet<Id>)
where
    Id: Copy + Eq + Hash,
    State: Default,
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
    tab_nodes: HashMap<RepositoryWorkspaceId, Vec<ProjectTreeTabNode>>,
    running_workspace_ids: HashSet<RepositoryWorkspaceId>,
    tab_spinner_states: HashMap<ProjectTreeTabId, SpinnerStateHandle>,
    repository_mouse_states: HashMap<RepositoryId, MouseStateHandle>,
    workspace_mouse_states: HashMap<RepositoryWorkspaceId, MouseStateHandle>,
    workspace_delete_mouse_states: HashMap<RepositoryWorkspaceId, MouseStateHandle>,
    workspace_add_tab_mouse_states: HashMap<RepositoryWorkspaceId, MouseStateHandle>,
    workspace_toggle_mouse_states: HashMap<RepositoryWorkspaceId, MouseStateHandle>,
    tab_mouse_states: HashMap<ProjectTreeTabId, MouseStateHandle>,
    tab_close_mouse_states: HashMap<ProjectTreeTabId, MouseStateHandle>,
    tab_model_chip_mouse_states: HashMap<ProjectTreeTabId, MouseStateHandle>,
    tab_status_mouse_states: HashMap<ProjectTreeTabId, MouseStateHandle>,
    repository_add_workspace_mouse_states: HashMap<RepositoryId, MouseStateHandle>,
    repository_remove_mouse_states: HashMap<RepositoryId, MouseStateHandle>,
    repository_drag_states: HashMap<RepositoryId, DraggableState>,
    repository_drag: Option<RepositoryDragPreview>,
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
            tab_nodes: HashMap::new(),
            running_workspace_ids: HashSet::new(),
            tab_spinner_states: HashMap::new(),
            repository_mouse_states: HashMap::new(),
            workspace_mouse_states: HashMap::new(),
            workspace_delete_mouse_states: HashMap::new(),
            workspace_add_tab_mouse_states: HashMap::new(),
            workspace_toggle_mouse_states: HashMap::new(),
            tab_mouse_states: HashMap::new(),
            tab_close_mouse_states: HashMap::new(),
            tab_model_chip_mouse_states: HashMap::new(),
            tab_status_mouse_states: HashMap::new(),
            repository_add_workspace_mouse_states: HashMap::new(),
            repository_remove_mouse_states: HashMap::new(),
            repository_drag_states: HashMap::new(),
            repository_drag: None,
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

    /// 设置各 workspace 的页签子节点,并同步呼吸环状态。
    pub fn set_tab_nodes(
        &mut self,
        tab_nodes: HashMap<RepositoryWorkspaceId, Vec<ProjectTreeTabNode>>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.tab_nodes == tab_nodes {
            return;
        }
        self.tab_nodes = tab_nodes;
        self.refresh_tree(ctx);
    }

    fn sync_spinner_states(&mut self) {
        let spinner_ids = self
            .tab_nodes
            .values()
            .flatten()
            .filter(|tab| tab.activity.needs_status_spinner())
            .map(|tab| tab.id)
            .collect::<HashSet<_>>();
        self.tab_spinner_states
            .retain(|id, _| spinner_ids.contains(id));
        for id in spinner_ids {
            self.tab_spinner_states.entry(id).or_default();
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

    pub fn reveal_workspace(
        &mut self,
        workspace_id: RepositoryWorkspaceId,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.state.reveal_workspace(workspace_id) {
            if let Some(repository_id) = self.state.repository_id_for_workspace(workspace_id) {
                ctx.emit(ProjectTreeEvent::RepositoryExpandedChanged {
                    repository_id,
                    expanded: true,
                });
            }
            ctx.emit(ProjectTreeEvent::WorkspaceExpandedChanged {
                workspace_id,
                expanded: true,
            });
            self.clipped_scroll_state.scroll_to_position(ScrollTarget {
                position_id: workspace_row_position_id(workspace_id),
                mode: ScrollToPositionMode::TopIntoView,
            });
            ctx.notify();
        }
    }

    fn refresh_tree(&mut self, ctx: &mut ViewContext<Self>) {
        let expanded_by_repository = self
            .state
            .repositories()
            .iter()
            .map(|repository| (repository.repository_id, repository.expanded))
            .collect::<HashMap<_, _>>();
        let expanded_by_workspace = self
            .state
            .repositories()
            .iter()
            .flat_map(|repository| repository.workspaces.iter())
            .map(|workspace| (workspace.workspace_id, workspace.expanded))
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

        self.state = ProjectTreeState::from_records(
            repositories,
            workspaces,
            &self.tab_counts,
            &self.tab_nodes,
        );
        for repository in &mut self.state.repositories {
            if let Some(expanded) = expanded_by_repository.get(&repository.repository_id) {
                repository.expanded = *expanded;
            }
            for workspace in &mut repository.workspaces {
                if let Some(expanded) = expanded_by_workspace.get(&workspace.workspace_id) {
                    workspace.expanded = *expanded;
                }
            }
        }
        self.state.set_active_workspace(selected_workspace_id);

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
        let tab_ids = self
            .tab_nodes
            .values()
            .flatten()
            .map(|tab| tab.id)
            .collect::<HashSet<_>>();
        synchronize_mouse_states(&mut self.repository_mouse_states, &repository_ids);
        synchronize_mouse_states(
            &mut self.repository_add_workspace_mouse_states,
            &repository_ids,
        );
        synchronize_mouse_states(&mut self.repository_remove_mouse_states, &repository_ids);
        synchronize_mouse_states(&mut self.repository_drag_states, &repository_ids);
        synchronize_mouse_states(&mut self.workspace_mouse_states, &workspace_ids);
        synchronize_mouse_states(&mut self.workspace_delete_mouse_states, &workspace_ids);
        synchronize_mouse_states(&mut self.workspace_add_tab_mouse_states, &workspace_ids);
        synchronize_mouse_states(&mut self.workspace_toggle_mouse_states, &workspace_ids);
        synchronize_mouse_states(&mut self.tab_mouse_states, &tab_ids);
        synchronize_mouse_states(&mut self.tab_close_mouse_states, &tab_ids);
        synchronize_mouse_states(&mut self.tab_model_chip_mouse_states, &tab_ids);
        synchronize_mouse_states(&mut self.tab_status_mouse_states, &tab_ids);
        self.running_workspace_ids
            .retain(|workspace_id| workspace_ids.contains(workspace_id));
        self.tab_nodes
            .retain(|workspace_id, _| workspace_ids.contains(workspace_id));
        self.sync_spinner_states();
        ctx.notify();
    }

    fn repositories_for_display(&self) -> Vec<&RepositoryTreeNode> {
        let repositories = self.state.repositories();
        let Some(preview) = &self.repository_drag else {
            return repositories.iter().collect();
        };
        let mut order: Vec<&RepositoryTreeNode> = repositories.iter().collect();
        if preview.from < order.len() && preview.to < order.len() && preview.from != preview.to {
            let moved = order.remove(preview.from);
            order.insert(preview.to, moved);
        }
        order
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
        ordered_ids: &[RepositoryId],
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
        let remove_action = ProjectTreeAction::DeleteRepository { repository_id };
        let remove_tooltip = appearance
            .ui_builder()
            .tool_tip("Remove repository".to_string())
            .build()
            .finish();
        let remove_repository = icon_button(
            appearance,
            icons::Icon::X,
            false,
            self.repository_remove_mouse_states
                .get(&repository_id)
                .expect("repository remove mouse state should be initialized during tree refresh")
                .clone(),
        )
        .with_tooltip(move || remove_tooltip)
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(remove_action.clone());
        })
        .with_cursor(Cursor::PointingHand)
        .finish();
        let remove_repository_position_id = repository_remove_position_id(repository_id);
        let remove_placeholder = ConstrainedBox::new(Empty::new().finish())
            .with_width(icons::ICON_DIMENSIONS)
            .with_height(icons::ICON_DIMENSIONS)
            .finish();

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

        let name = Text::new_inline(
            repository.display_name.clone(),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_clip(ClipConfig::ellipsis())
        .with_color(theme.main_text_color(theme.background()).into())
        .finish();
        let chevron_icon = tree_row_icon(chevron, icon_color, TREE_CHEVRON_SIZE);
        let folder_icon = tree_row_icon(icons::Icon::Folder, icon_color, TREE_ROW_ICON_SIZE);
        let toggle_action = ProjectTreeAction::ToggleRepository { repository_id };

        let hoverable = Hoverable::new(
            self.repository_mouse_states
                .get(&repository_id)
                .expect("repository row mouse state should be initialized during tree refresh")
                .clone(),
            move |mouse_state| {
                let show_remove = should_show_repository_hover_actions(mouse_state.is_hovered());
                let remove_repository = if show_remove {
                    remove_repository
                } else {
                    remove_placeholder
                };
                let remove_repository =
                    SavePosition::new(remove_repository, &remove_repository_position_id).finish();
                let labeled = Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(TREE_ICON_GAP)
                    .with_child(chevron_icon)
                    .with_child(folder_icon)
                    .with_child(Shrinkable::new(1.0, name).finish())
                    .finish();
                let trailing = Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(TREE_TRAILING_ACTION_SPACING)
                    .with_child(workspace_count)
                    .with_child(add_workspace)
                    .with_child(remove_repository)
                    .finish();
                let row = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Shrinkable::new(1.0, labeled).finish())
                    .with_child(trailing)
                    .finish();
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
        .finish();

        let source_id = repository_id;
        let ordered_ids = ordered_ids.to_vec();
        Draggable::new(
            self.repository_drag_states
                .get(&repository_id)
                .expect("repository drag state should be initialized during tree refresh")
                .clone(),
            hoverable,
        )
        .with_drag_axis(DragAxis::VerticalOnly)
        .with_accepted_by_drop_target_fn(move |data, _| {
            if data
                .as_any()
                .downcast_ref::<RepositoryDropTarget>()
                .is_some_and(|target| target.repository_id != source_id)
            {
                AcceptedByDropTarget::Yes
            } else {
                AcceptedByDropTarget::No
            }
        })
        .on_drag_start(move |ctx, _, _| {
            let Some(from) = ordered_ids.iter().position(|id| *id == source_id) else {
                return;
            };
            let Some(centers) = repository_block_centers(ctx, &ordered_ids) else {
                return;
            };
            ctx.dispatch_typed_action(ProjectTreeAction::RepositoryDragStarted {
                source_id,
                from,
                centers,
            });
        })
        .on_drag(move |ctx, _, drag_position, _| {
            ctx.dispatch_typed_action(ProjectTreeAction::RepositoryDragMoved {
                pointer_y: drag_position.center().y(),
            });
        })
        .on_drop(move |ctx, _, drag_position, _| {
            ctx.dispatch_typed_action(ProjectTreeAction::RepositoryDragDropped {
                pointer_y: drag_position.center().y(),
            });
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
        ConstrainedBox::new(dot)
            .with_width(WORKSPACE_RUNNING_DOT_SIZE)
            .with_height(WORKSPACE_RUNNING_DOT_SIZE)
            .finish()
    }

    fn render_workspace_activity_slot(
        &self,
        visual_state: WorkspaceVisualState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        match visual_state.activity_slot() {
            WorkspaceActivitySlot::Empty | WorkspaceActivitySlot::Agent(_) => {
                sized_activity_slot(Empty::new().finish())
            }
            WorkspaceActivitySlot::RunningDot => {
                sized_activity_slot(Self::render_workspace_running_dot(visual_state, appearance))
            }
        }
    }

    fn render_tab_activity_slot(
        &self,
        tab: &ProjectTreeTabNode,
        visual_state: WorkspaceVisualState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        match &tab.activity {
            TabNodeActivity::Idle => tree_status_slot(
                ConstrainedBox::new(
                    Image::new(
                        AssetSource::Bundled {
                            path: ITERM_PROMPT_ICON_PATH,
                        },
                        CacheOption::BySize,
                    )
                    .finish(),
                )
                .with_width(TREE_ROW_ICON_SIZE)
                .with_height(TREE_ROW_ICON_SIZE)
                .finish(),
            ),
            TabNodeActivity::RunningDot => {
                tree_status_slot(Self::render_workspace_running_dot(visual_state, appearance))
            }
            TabNodeActivity::Agent(activity) => {
                self.render_tab_agent_leading(tab.id, activity.clone(), appearance)
            }
        }
    }

    /// Orca 行序: [状态点] [品牌图标],状态与身份分离,无绕头像呼吸环。
    fn render_tab_agent_leading(
        &self,
        tab_id: ProjectTreeTabId,
        activity: WorkspaceAgentActivity,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        // Compute-on-read Done→Idle so glyph matches Idle after residual TTL.
        let status = self.render_agent_status_glyph(tab_id, &activity, appearance);
        let brand = render_agent_brand_icon(activity.identity, theme);
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(TAB_AGENT_LEADING_GAP)
            .with_child(status)
            .with_child(brand)
            .finish()
    }

    fn render_agent_status_glyph(
        &self,
        tab_id: ProjectTreeTabId,
        activity: &WorkspaceAgentActivity,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let phase = activity.presentation_phase();
        let color = agent_status_dot_color(phase, theme);
        // Done with a timestamp: self-refreshing glyph flips to Idle at 24h without View::render.
        let status_slot: Box<dyn Element> =
            if activity.phase == WorkspaceAgentPhase::Done {
                if let Some(since) = activity.phase_changed_at {
                    Box::new(DemotingDoneStatusGlyph::new(
                        since,
                        agent_status_dot_color(WorkspaceAgentPhase::Idle, theme),
                        theme.ansi_fg_green(),
                        TAB_AGENT_STATUS_SLOT_SIZE,
                    ))
                } else {
                    // No timestamp → keep static Done check (cannot age out).
                    ConstrainedBox::new(
                        Flex::row()
                            .with_main_axis_size(MainAxisSize::Max)
                            .with_main_axis_alignment(MainAxisAlignment::Center)
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_child(
                                ConstrainedBox::new(
                                    WarpIcon::Check
                                        .to_warpui_icon(ThemeFill::Solid(theme.ansi_fg_green()))
                                        .finish(),
                                )
                                .with_width(11.)
                                .with_height(11.)
                                .finish(),
                            )
                            .finish(),
                    )
                    .with_width(TAB_AGENT_STATUS_SLOT_SIZE)
                    .with_height(TAB_AGENT_STATUS_SLOT_SIZE)
                    .finish()
                }
            } else {
                let glyph: Box<dyn Element> = match phase {
                    WorkspaceAgentPhase::InProgress => {
                        let handle = self
                            .tab_spinner_states
                            .get(&tab_id)
                            .expect("InProgress agent 必须先由 sync_spinner_states 插入 spinner handle")
                            .clone();
                        Box::new(BrailleSpinner::new(
                            appearance.ui_font_family(),
                            10.,
                            color,
                            handle,
                        ))
                    }
                    WorkspaceAgentPhase::Done => ConstrainedBox::new(
                        WarpIcon::Check
                            .to_warpui_icon(ThemeFill::Solid(theme.ansi_fg_green()))
                            .finish(),
                    )
                    .with_width(11.)
                    .with_height(11.)
                    .finish(),
                    WorkspaceAgentPhase::Waiting => Text::new_inline(
                        "?",
                        appearance.ui_font_family(),
                        appearance.ui_font_footnote(),
                    )
                    .with_color(color.into())
                    .finish(),
                    WorkspaceAgentPhase::Idle
                    | WorkspaceAgentPhase::Blocked
                    | WorkspaceAgentPhase::Failed => ConstrainedBox::new(
                        Container::new(Empty::new().finish())
                            .with_background(color)
                            .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                            .finish(),
                    )
                    .with_width(6.)
                    .with_height(6.)
                    .finish(),
                };
                ConstrainedBox::new(
                    Flex::row()
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_main_axis_alignment(MainAxisAlignment::Center)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(glyph)
                        .finish(),
                )
                .with_width(TAB_AGENT_STATUS_SLOT_SIZE)
                .with_height(TAB_AGENT_STATUS_SLOT_SIZE)
                .finish()
            };
        // 行内只用图形;英文状态词走 hover tooltip(对齐 Orca AgentStateDot title)。
        let status_mouse = self
            .tab_status_mouse_states
            .get(&tab_id)
            .expect("tab status mouse state should be initialized during tree refresh")
            .clone();
        appearance.ui_builder().overlay_tool_tip_on_element(
            phase.accessibility_label().to_owned(),
            status_mouse,
            status_slot,
            ParentAnchor::BottomMiddle,
            ChildAnchor::TopMiddle,
            vec2f(0., 4.),
        )
    }

    fn render_workspace_tab_count(
        tab_count: usize,
        visual_state: WorkspaceVisualState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let color = if visual_state.should_emphasize_workspace_label() {
            // Depth Stack: brighter meta inside selected group (not accent).
            DEPTH_STACK_HEAD_LABEL
        } else {
            theme
                .sub_text_color(theme.background())
                .into_solid_bias_right_color()
        };
        render_tree_trailing_meta(
            Text::new_inline(
                tab_count_badge_label(tab_count),
                appearance.ui_font_family(),
                appearance.ui_font_footnote(),
            )
            .with_color(color.into())
            .finish(),
        )
    }

    fn render_workspace_row(
        &self,
        workspace: &WorkspaceTreeNode,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let selected = workspace_group_is_selected(
            self.state.selected_workspace_id(),
            workspace.workspace_id,
        );
        // Depth Stack: no per-row accent; elevated group chrome wraps the row.
        debug_assert!(!workspace_row_shows_selection_accent(workspace, selected));
        let label_color = if selected {
            DEPTH_STACK_HEAD_LABEL
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

        let any_child_busy = workspace.tabs.iter().any(|tab| tab.activity.is_busy());
        let parent_slot = workspace_parent_activity_slot(workspace.expanded, any_child_busy);
        let visual_state = WorkspaceVisualState::new(
            selected,
            matches!(parent_slot, WorkspaceActivitySlot::RunningDot),
            None,
        );
        let activity_slot = self.render_workspace_activity_slot(visual_state.clone(), appearance);
        let tab_count =
            Self::render_workspace_tab_count(workspace.tab_count, visual_state.clone(), appearance);
        let toggle_action = ProjectTreeAction::ToggleWorkspace { workspace_id };
        let chevron_icon = if workspace.expanded {
            icons::Icon::ChevronDown
        } else {
            icons::Icon::ChevronRight
        };
        let chevron = Hoverable::new(
            self.workspace_toggle_mouse_states
                .get(&workspace_id)
                .expect("workspace toggle mouse state should be initialized during tree refresh")
                .clone(),
            move |_| tree_row_icon(chevron_icon, metadata_color, TREE_CHEVRON_SIZE),
        )
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(toggle_action.clone());
        })
        .finish();
        let workspace_icon = if matches!(parent_slot, WorkspaceActivitySlot::RunningDot) {
            activity_slot
        } else {
            tree_row_icon(icons::Icon::GitBranch, metadata_color, TREE_ROW_ICON_SIZE)
        };
        let labeled_content = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(TREE_ICON_GAP)
            .with_child(chevron)
            .with_child(workspace_icon)
            .with_child(Shrinkable::new(1.0, content).finish())
            .finish();

        let delete_tooltip = appearance
            .ui_builder()
            .tool_tip("Remove workspace".to_string())
            .build()
            .finish();
        let delete = icon_button(
            appearance,
            icons::Icon::X,
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
        let new_tab_action = ProjectTreeAction::NewTab { workspace_id };
        let new_tab_tooltip = appearance
            .ui_builder()
            .tool_tip("New tab".to_string())
            .build()
            .finish();
        let new_tab = icon_button(
            appearance,
            icons::Icon::Plus,
            false,
            self.workspace_add_tab_mouse_states
                .get(&workspace_id)
                .expect("workspace add-tab mouse state should be initialized during tree refresh")
                .clone(),
        )
        .with_tooltip(move || new_tab_tooltip)
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(new_tab_action.clone());
        })
        .with_cursor(Cursor::PointingHand)
        .finish();
        let new_tab_placeholder = ConstrainedBox::new(Empty::new().finish())
            .with_width(icons::ICON_DIMENSIONS)
            .with_height(icons::ICON_DIMENSIONS)
            .finish();

        Hoverable::new(
            self.workspace_mouse_states
                .get(&workspace_id)
                .expect("workspace row mouse state should be initialized during tree refresh")
                .clone(),
            move |mouse_state| {
                let show_actions = should_show_workspace_hover_actions(mouse_state.is_hovered());
                let new_tab = if show_actions {
                    new_tab
                } else {
                    new_tab_placeholder
                };
                let delete = if show_actions {
                    delete
                } else {
                    delete_placeholder
                };
                let actions = render_tree_trailing_actions(
                    Flex::row()
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_main_axis_alignment(MainAxisAlignment::End)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_spacing(TREE_TRAILING_ACTION_SPACING)
                        .with_child(new_tab)
                        .with_child(delete)
                        .finish(),
                );
                let row_content = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Shrinkable::new(1.0, labeled_content).finish())
                    .with_child(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_spacing(TREE_TRAILING_ACTION_SPACING)
                            .with_child(tab_count)
                            .with_child(actions)
                            .finish(),
                    )
                    .finish();
                let mut row_container = Container::new(row_content)
                    .with_horizontal_padding(DEPTH_STACK_ROW_INSET_H)
                    .with_vertical_padding(5.)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(5.)));
                // Depth Stack: never paint row-level accent fill/frame on the head.
                debug_assert!(!visual_state.should_render_selection_accent());
                debug_assert!(!visual_state.should_render_selection_frame());
                if mouse_state.is_hovered() {
                    row_container = row_container.with_background(theme.surface_overlay_2());
                } else if visual_state.should_fill_idle_row() {
                    row_container = row_container.with_background(theme.surface_overlay_1());
                }

                row_container.finish()
            },
        )
        .with_cursor(Cursor::PointingHand)
        .with_defer_events_to_children()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish()
    }

    fn render_tab_row(
        &self,
        workspace_id: RepositoryWorkspaceId,
        tab: &ProjectTreeTabNode,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let group_selected = workspace_group_is_selected(
            self.state.selected_workspace_id(),
            workspace_id,
        );
        let visual_state = WorkspaceVisualState::new(
            tab.is_active,
            matches!(tab.activity, TabNodeActivity::RunningDot),
            tab.activity.agent().cloned(),
        );
        // Depth Stack: active tab = near-white on chip; inactive in selected group = secondary.
        let label_color = if tab.is_active {
            DEPTH_STACK_ACTIVE_TAB_LABEL
        } else if group_selected {
            DEPTH_STACK_INACTIVE_TAB_LABEL
        } else {
            theme
                .main_text_color(theme.background())
                .into_solid_bias_right_color()
        };
        let activity_slot = self.render_tab_activity_slot(tab, visual_state.clone(), appearance);
        let title = Text::new_inline(
            tab.title.clone(),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_clip(ClipConfig::ellipsis())
        .with_color(label_color.into())
        .finish();
        let sub_text = theme
            .sub_text_color(theme.background())
            .into_solid_bias_right_color();
        let model_chip = tab.activity.agent().and_then(|activity| {
            let label = activity.model_chip_label()?;
            let tooltip = activity
                .model_chip_tooltip()
                .unwrap_or(label.as_str())
                .to_owned();
            // Secondary type only — never a gray pill competing with the inset.
            let model_color = if tab.is_active {
                DEPTH_STACK_MODEL_LABEL
            } else {
                sub_text
            };
            let chip = ConstrainedBox::new(
                Text::new_inline(
                    label,
                    appearance.monospace_font_family(),
                    appearance.ui_font_footnote(),
                )
                .with_clip(ClipConfig::ellipsis())
                .with_color(model_color.into())
                .finish(),
            )
            .with_max_width(MODEL_CHIP_MAX_WIDTH)
            .finish();
            // Orca CompactAgentRow: title={model} on the truncated chip.
            let chip_mouse = self
                .tab_model_chip_mouse_states
                .get(&tab.id)
                .expect("tab model chip mouse state should be initialized during tree refresh")
                .clone();
            Some(appearance.ui_builder().overlay_tool_tip_on_element(
                tooltip,
                chip_mouse,
                chip,
                ParentAnchor::BottomMiddle,
                ChildAnchor::TopMiddle,
                vec2f(0., 4.),
            ))
        });
        let relative_time = tab.activity.agent().and_then(|activity| {
            activity.phase_changed_at.map(|since| {
                render_tree_trailing_meta(Box::new(RelativeTimeLabel::new(
                    since,
                    appearance.ui_font_family(),
                    appearance.ui_font_footnote(),
                    sub_text,
                )))
            })
        });
        let labeled_content = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(TREE_ICON_GAP)
            .with_child(
                ConstrainedBox::new(Empty::new().finish())
                    .with_width(TAB_UNDER_WORKSPACE_INDENT)
                    .finish(),
            )
            .with_child(activity_slot)
            .with_child(Shrinkable::new(1.0, title).finish())
            .finish();

        let tab_id = tab.id;
        let select_action = ProjectTreeAction::SelectTab {
            workspace_id,
            tab_id,
        };
        let close_action = ProjectTreeAction::CloseTab {
            workspace_id,
            tab_id,
        };
        let close_tooltip = appearance
            .ui_builder()
            .tool_tip("Close tab".to_string())
            .build()
            .finish();
        let close = icon_button(
            appearance,
            icons::Icon::X,
            false,
            self.tab_close_mouse_states
                .get(&tab_id)
                .expect("tab close mouse state should be initialized during tree refresh")
                .clone(),
        )
        .with_tooltip(move || close_tooltip)
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(close_action.clone());
        })
        .with_cursor(Cursor::PointingHand)
        .finish();
        let close_placeholder = ConstrainedBox::new(Empty::new().finish())
            .with_width(icons::ICON_DIMENSIONS)
            .with_height(icons::ICON_DIMENSIONS)
            .finish();

        Hoverable::new(
            self.tab_mouse_states
                .get(&tab_id)
                .expect("tab row mouse state should be initialized during tree refresh")
                .clone(),
            move |mouse_state| {
                let close = if mouse_state.is_hovered() {
                    close
                } else {
                    close_placeholder
                };
                // Match workspace trailing: [model?][meta][2-slot actions] so
                // relative time and tab-count share one right-edge column.
                let actions = render_tree_trailing_actions(
                    Flex::row()
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_main_axis_alignment(MainAxisAlignment::End)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(close)
                        .finish(),
                );
                let mut trailing = Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(TREE_TRAILING_ACTION_SPACING);
                if let Some(model_chip) = model_chip {
                    trailing = trailing.with_child(model_chip);
                }
                if let Some(relative_time) = relative_time {
                    trailing = trailing.with_child(relative_time);
                }
                trailing = trailing.with_child(actions);
                let row_content = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Shrinkable::new(1.0, labeled_content).finish())
                    .with_child(trailing.finish())
                    .finish();
                // Depth Stack: active inset ≠ hover wash (see depth_stack_tab_fill).
                let fill = depth_stack_tab_fill(
                    visual_state.should_render_active_tab_chip(),
                    mouse_state.is_hovered(),
                );
                let row_el: Box<dyn Element> = match fill {
                    DepthStackTabFill::ActiveInset => depth_stack_active_inset_slot(row_content),
                    DepthStackTabFill::HoverWash => Container::new(row_content)
                        .with_horizontal_padding(DEPTH_STACK_ROW_INSET_H)
                        .with_vertical_padding(5.)
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                            DEPTH_STACK_TAB_HOVER_RADIUS,
                        )))
                        // Light sweep only — no border, no shadow, not overlay_2 gray pill.
                        .with_background(DEPTH_STACK_TAB_HOVER_WASH)
                        .with_horizontal_margin(DEPTH_STACK_TAB_GUTTER_H)
                        .finish(),
                    DepthStackTabFill::None => Container::new(row_content)
                        .with_horizontal_padding(DEPTH_STACK_ROW_INSET_H)
                        .with_vertical_padding(5.)
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                            DEPTH_STACK_TAB_HOVER_RADIUS,
                        )))
                        .with_horizontal_margin(DEPTH_STACK_TAB_GUTTER_H)
                        .finish(),
                };

                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Shrinkable::new(1.0, row_el).finish())
                    .finish()
            },
        )
        .with_cursor(Cursor::PointingHand)
        .with_defer_events_to_children()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(select_action.clone());
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
            ProjectTreeAction::DeleteRepository { repository_id } => {
                ctx.emit(ProjectTreeEvent::DeleteRepositoryRequested {
                    repository_id: *repository_id,
                });
            }
            ProjectTreeAction::DeleteWorkspace { workspace_id } => {
                ctx.emit(ProjectTreeEvent::DeleteWorkspaceRequested {
                    workspace_id: *workspace_id,
                });
            }
            ProjectTreeAction::ToggleRepository { repository_id } => {
                if self.state.toggle_repository(*repository_id) {
                    if let Some(expanded) = self.state.repository_expanded(*repository_id) {
                        ctx.emit(ProjectTreeEvent::RepositoryExpandedChanged {
                            repository_id: *repository_id,
                            expanded,
                        });
                    }
                    ctx.notify();
                }
            }
            ProjectTreeAction::SelectWorkspace { workspace_id } => {
                if let Some(workspace_id) = workspace_id {
                    let was_expanded = self.state.workspace_is_expanded(*workspace_id);
                    self.state.select_or_toggle_workspace(*workspace_id);
                    if let Some(expanded) = self.state.workspace_expanded(*workspace_id) {
                        if expanded != was_expanded {
                            ctx.emit(ProjectTreeEvent::WorkspaceExpandedChanged {
                                workspace_id: *workspace_id,
                                expanded,
                            });
                        }
                    }
                } else {
                    self.state.selected_workspace_id = None;
                }
                ctx.emit(ProjectTreeEvent::WorkspaceSelected {
                    workspace_id: *workspace_id,
                });
                ctx.notify();
            }
            ProjectTreeAction::ToggleWorkspace { workspace_id } => {
                if self.state.toggle_workspace_expanded(*workspace_id) {
                    if let Some(expanded) = self.state.workspace_expanded(*workspace_id) {
                        ctx.emit(ProjectTreeEvent::WorkspaceExpandedChanged {
                            workspace_id: *workspace_id,
                            expanded,
                        });
                    }
                    ctx.notify();
                }
            }
            ProjectTreeAction::SelectTab {
                workspace_id,
                tab_id,
            } => {
                let was_expanded = self.state.workspace_is_expanded(*workspace_id);
                self.state.select_workspace(*workspace_id);
                if !was_expanded {
                    ctx.emit(ProjectTreeEvent::WorkspaceExpandedChanged {
                        workspace_id: *workspace_id,
                        expanded: true,
                    });
                }
                ctx.emit(ProjectTreeEvent::TabSelected {
                    workspace_id: *workspace_id,
                    tab_id: *tab_id,
                });
                ctx.notify();
            }
            ProjectTreeAction::CloseTab {
                workspace_id,
                tab_id,
            } => {
                ctx.emit(ProjectTreeEvent::TabCloseRequested {
                    workspace_id: *workspace_id,
                    tab_id: *tab_id,
                });
            }
            ProjectTreeAction::NewTab { workspace_id } => {
                let was_expanded = self.state.workspace_is_expanded(*workspace_id);
                self.state.select_workspace(*workspace_id);
                if !was_expanded {
                    ctx.emit(ProjectTreeEvent::WorkspaceExpandedChanged {
                        workspace_id: *workspace_id,
                        expanded: true,
                    });
                }
                ctx.emit(ProjectTreeEvent::NewTabRequested {
                    workspace_id: *workspace_id,
                });
                ctx.notify();
            }
            ProjectTreeAction::ReorderTabs {
                workspace_id,
                from,
                to,
            } => {
                ctx.emit(ProjectTreeEvent::TabsReordered {
                    workspace_id: *workspace_id,
                    from: *from,
                    to: *to,
                });
            }
            ProjectTreeAction::RepositoryDragStarted {
                source_id,
                from,
                centers,
            } => {
                self.repository_drag = Some(RepositoryDragPreview {
                    source_id: *source_id,
                    from: *from,
                    to: *from,
                    centers: centers.clone(),
                });
                ctx.notify();
            }
            ProjectTreeAction::RepositoryDragMoved { pointer_y } => {
                let Some(preview) = &mut self.repository_drag else {
                    return;
                };
                let to = repository_insert_index_for_centers(&preview.centers, *pointer_y);
                if preview.to != to {
                    preview.to = to;
                    ctx.notify();
                }
            }
            ProjectTreeAction::RepositoryDragDropped { pointer_y } => {
                let Some(preview) = self.repository_drag.take() else {
                    return;
                };
                let source_matches = self
                    .state
                    .repositories
                    .get(preview.from)
                    .is_some_and(|repository| repository.repository_id == preview.source_id);
                let to = repository_insert_index_for_centers(&preview.centers, *pointer_y);
                if source_matches
                    && preview.from != to
                    && preview.from < self.state.repositories.len()
                    && to < self.state.repositories.len()
                {
                    let moved = self.state.repositories.remove(preview.from);
                    self.state.repositories.insert(to, moved);
                    ctx.emit(ProjectTreeEvent::RepositoriesReordered {
                        from: preview.from,
                        to,
                    });
                }
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
        let ordered_ids = self
            .state
            .repositories()
            .iter()
            .map(|repository| repository.repository_id)
            .collect::<Vec<_>>();
        for repository in self.repositories_for_display() {
            let repository_id = repository.repository_id;
            let mut repository_group = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(self.render_repository_row(repository, &ordered_ids, appearance));
            if repository.expanded && !repository.workspaces.is_empty() {
                let mut workspaces = Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .with_spacing(0.);
                for workspace in &repository.workspaces {
                    let group_selected = workspace_group_is_selected(
                        self.state.selected_workspace_id(),
                        workspace.workspace_id,
                    );
                    let mut group = Flex::column()
                        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .with_spacing(0.);
                    group.add_child(
                        SavePosition::new(
                            self.render_workspace_row(workspace, appearance),
                            &workspace_row_position_id(workspace.workspace_id),
                        )
                        .finish(),
                    );
                    if workspace.expanded {
                        for tab in &workspace.tabs {
                            group.add_child(self.render_tab_row(
                                workspace.workspace_id,
                                tab,
                                appearance,
                            ));
                        }
                    }
                    let group_el = group.finish();
                    // Depth Stack: selected workspace = one elevated card around head + tabs.
                    if group_selected {
                        workspaces.add_child(depth_stack_selected_group(group_el));
                    } else {
                        workspaces.add_child(group_el);
                    }
                }
                repository_group.add_child(
                    Container::new(workspaces.finish())
                        .with_margin_left(WORKSPACE_GROUP_INDENT)
                        .with_margin_top(2.)
                        .finish(),
                );
            }
            // DropTarget 包在 Container 上,避免 Draggable 把 bounds 带到幽灵位置,
            // 导致松手永远命中自己、顺序弹回。见 ssh_manager/panel.rs 同类嵌套。
            let stable_anchor = Container::new(repository_group.finish()).finish();
            let drop_target =
                DropTarget::new(stable_anchor, RepositoryDropTarget { repository_id }).finish();
            tree.add_child(
                SavePosition::new(drop_target, &repository_block_position_id(repository_id))
                    .finish(),
            );
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
