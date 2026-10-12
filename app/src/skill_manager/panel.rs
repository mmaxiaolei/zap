use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ai::skills::SkillProvider;
use warp_core::ui::appearance::Appearance;
use warp_core::ui::color::CLAUDE_ORANGE;
use warp_core::ui::icons::Icon as WarpIcon;
use warp_core::ui::theme::color::internal_colors;
use warp_core::ui::theme::WarpTheme;
use warpui::{
    color::ColorU,
    elements::{
        Border, ChildAnchor, ChildView, Clipped, ClippedScrollStateHandle, ClippedScrollable,
        ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Element, Empty,
        Fill as ElementFill, Flex, Hoverable, MainAxisSize, MouseStateHandle, OffsetPositioning,
        Padding, ParentAnchor, ParentElement, ParentOffsetBounds, Radius, SavePosition,
        ScrollTarget, ScrollToPositionMode, ScrollbarWidth, Shrinkable, Stack, Text, Wrap,
    },
    geometry::vector::vec2f,
    platform::Cursor,
    text_layout::ClipConfig,
    AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use crate::ai::skills::{
    SkillInventoryDuplicate, SkillInventoryItem, SkillManager, SkillManagerEvent,
};
use crate::editor::{
    EditorOptions, EditorView, Event as EditorEvent, PropagateAndNoOpNavigationKeys,
    PropagateHorizontalNavigationKeys, TextOptions,
};
use crate::view_components::dropdown::{Dropdown, DropdownItem};

const PANEL_PADDING: f32 = 8.0;
const ROW_PADDING_VERTICAL: f32 = 5.0;
const ROW_PADDING_HORIZONTAL: f32 = 8.0;
// filter tab:保持紧凑高度,用 12px 字号 + 强对比激活态撑起可读性,
// 避免被外壳 ClippedScrollable 拦截点击事件。
const FILTER_BUTTON_HEIGHT: f32 = 24.0;
const FILTER_BUTTON_HORIZONTAL_PADDING: f32 = 8.0;
const FILTER_BUTTON_CORNER_RADIUS: f32 = 4.0;
// 来源 tab 平铺显示的最大数量;超过则折叠为「全部 + 来源下拉」。
// 取 4:典型侧栏宽度下「全部」加约 4 个短来源名仍可平铺。布局引擎是单遍保留树,
// 无法按实测像素宽度自适应折叠,故以来源数量作为折叠阈值。
const FILTER_TABS_INLINE_MAX: usize = 4;
const SOURCE_TAG_HEIGHT: f32 = 20.0;
const SOURCE_TAG_PIP: f32 = 6.0;
const SOURCE_TAG_CHECK: f32 = 10.0;

#[derive(Clone, Debug)]
pub enum SkillManagerPanelAction {
    SelectProviderFilter(Option<SkillProvider>),
    EditSkill(PathBuf),
    /// 点来源标签. `open` 为 true 时同时打开该文件.
    PinSkillSource {
        name: String,
        path: PathBuf,
        open: bool,
    },
}

#[derive(Clone, Debug)]
pub enum SkillManagerPanelEvent {
    OpenSkillFile { path: PathBuf },
}

pub struct SkillManagerPanel {
    selected_path: Option<PathBuf>,
    /// 每个 skill 名字当前要打开的那一份. 缺省时用生效的那份.
    pinned_paths: HashMap<String, PathBuf>,
    provider_filter: Option<SkillProvider>,
    query_editor: ViewHandle<EditorView>,
    // 折叠态(来源数 > FILTER_TABS_INLINE_MAX)收纳各来源 tab 的「来源」下拉。
    source_dropdown: ViewHandle<Dropdown<SkillManagerPanelAction>>,
    filter_mouse_states: RefCell<HashMap<Option<SkillProvider>, MouseStateHandle>>,
    row_mouse_states: RefCell<HashMap<PathBuf, MouseStateHandle>>,
    tag_mouse_states: RefCell<HashMap<PathBuf, MouseStateHandle>>,
    list_scroll_state: ClippedScrollStateHandle,
}

impl SkillManagerPanel {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let query_editor = ctx.add_typed_action_view(|ctx| {
            let options = EditorOptions {
                text: TextOptions::ui_text(
                    Some(Appearance::as_ref(ctx).ui_font_subheading()),
                    Appearance::as_ref(ctx),
                ),
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::AtBoundary,
                propagate_horizontal_navigation_keys: PropagateHorizontalNavigationKeys::Always,
                single_line: true,
                clear_selections_on_blur: true,
                convert_newline_to_space: true,
                ..Default::default()
            };
            let mut editor = EditorView::new(options, ctx);
            editor.set_placeholder_text(crate::t!("skill-manager-search-placeholder"), ctx);
            editor
        });

        ctx.subscribe_to_view(&query_editor, |me, _, event, ctx| {
            if matches!(
                event,
                EditorEvent::Edited(_)
                    | EditorEvent::BufferReplaced
                    | EditorEvent::BufferReinitialized
            ) {
                me.scroll_selected_path_into_view_with_ctx(ctx);
                ctx.notify();
            }
        });

        ctx.subscribe_to_model(&Appearance::handle(ctx), |_, _, _, ctx| {
            ctx.notify();
        });

        let skill_manager = SkillManager::handle(ctx);
        ctx.subscribe_to_model(&skill_manager, |me, _manager, event, ctx| match event {
            SkillManagerEvent::InventoryChanged => {
                let inventory = SkillManager::as_ref(ctx).list_skill_inventory(ctx);
                if me.provider_filter.is_some_and(|provider| {
                    !Self::providers_in_inventory(&inventory).contains(&provider)
                }) {
                    me.provider_filter = None;
                }
                let query = me.query(ctx);
                let items = Self::filter_inventory(&inventory, &query, me.provider_filter);
                me.scroll_selected_path_into_view(&items);
                // 来源集合随 inventory 变化,重建折叠态下拉的选项与选中态。
                me.rebuild_source_dropdown(ctx);
                ctx.notify();
            }
        });

        // 折叠态使用的「来源」下拉。由本面板的 ViewContext 创建,其派发的
        // SelectProviderFilter 会冒泡回本面板的 handle_action。
        let source_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            // 下拉头紧贴文本宽度,与左侧「全部」按钮并排,不撑满整行。
            dropdown.set_main_axis_size(MainAxisSize::Min, ctx);
            // 选中「全部」项时,闭合态显示「来源」占位,避免与左侧独立「全部」按钮重复。
            let all_label = crate::t!("skill-manager-filter-all").to_string();
            let source_label = crate::t!("skill-manager-filter-provider").to_string();
            dropdown.set_menu_header_text_override(move |label| {
                if label == all_label {
                    source_label.clone()
                } else {
                    label.to_string()
                }
            });
            dropdown
        });

        let mut me = Self {
            selected_path: None,
            pinned_paths: HashMap::new(),
            provider_filter: None,
            query_editor,
            source_dropdown,
            filter_mouse_states: RefCell::new(HashMap::new()),
            row_mouse_states: RefCell::new(HashMap::new()),
            tag_mouse_states: RefCell::new(HashMap::new()),
            list_scroll_state: ClippedScrollStateHandle::default(),
        };
        me.rebuild_source_dropdown(ctx);
        me
    }

    /// 重建「来源」下拉的选项与选中态。
    ///
    /// 首项「全部」对应清空过滤(index 0),其余项依次对应当前 inventory 中存在的
    /// 各 provider。选中态由 `provider_filter` 驱动,与左侧「全部」按钮保持一致。
    fn rebuild_source_dropdown(&mut self, ctx: &mut ViewContext<Self>) {
        let inventory = SkillManager::as_ref(ctx).list_skill_inventory(ctx);
        let providers = Self::providers_in_inventory(&inventory);

        let mut items = vec![DropdownItem::new(
            crate::t!("skill-manager-filter-all"),
            SkillManagerPanelAction::SelectProviderFilter(None),
        )];
        for provider in &providers {
            items.push(DropdownItem::new(
                provider.to_string(),
                SkillManagerPanelAction::SelectProviderFilter(Some(*provider)),
            ));
        }

        // 首项「全部」占据 index 0,故命中的 provider 需 +1。
        let selected_index = match self.provider_filter {
            None => 0,
            Some(filter) => providers
                .iter()
                .position(|provider| *provider == filter)
                .map_or(0, |position| position + 1),
        };

        self.source_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_items(items, ctx);
            dropdown.set_selected_by_index(selected_index, ctx);
        });
    }

    fn query(&self, app: &AppContext) -> String {
        self.query_editor
            .as_ref(app)
            .buffer_text(app)
            .trim()
            .to_lowercase()
    }

    fn providers_in_inventory(inventory: &[SkillInventoryItem]) -> Vec<SkillProvider> {
        let mut providers = inventory
            .iter()
            .flat_map(|item| item.duplicates.iter().map(|duplicate| duplicate.provider))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        providers.sort_by_key(|provider| provider.to_string());
        providers
    }

    fn filter_inventory(
        inventory: &[SkillInventoryItem],
        query: &str,
        provider_filter: Option<SkillProvider>,
    ) -> Vec<SkillInventoryItem> {
        inventory
            .iter()
            .filter_map(|item| {
                let duplicates = item
                    .duplicates
                    .iter()
                    .filter(|duplicate| {
                        provider_filter.is_none_or(|provider| duplicate.provider == provider)
                            && (query.is_empty()
                                || duplicate.name.to_lowercase().contains(query)
                                || duplicate.description.to_lowercase().contains(query)
                                || duplicate
                                    .path
                                    .display()
                                    .to_string()
                                    .to_lowercase()
                                    .contains(query))
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                if duplicates.is_empty() {
                    return None;
                }
                // 过滤后仍保留真正生效的那份, 避免「默认」被换成剩余列表的第一项.
                let default_skill = duplicates
                    .iter()
                    .find(|duplicate| duplicate.path == item.default_skill.path)
                    .cloned()
                    .unwrap_or_else(|| duplicates[0].clone());
                Some(SkillInventoryItem {
                    name: item.name.clone(),
                    default_skill,
                    duplicates,
                })
            })
            .collect()
    }

    /// 悬停中的来源优先, 其次是点选记住的那份, 最后是生效的那份.
    fn resolve_shown_duplicate<'a>(
        item: &'a SkillInventoryItem,
        pinned_path: Option<&Path>,
        hovered_path: Option<&Path>,
    ) -> &'a SkillInventoryDuplicate {
        if let Some(path) = hovered_path {
            if let Some(duplicate) = item
                .duplicates
                .iter()
                .find(|duplicate| duplicate.path.as_path() == path)
            {
                return duplicate;
            }
        }
        if let Some(path) = pinned_path {
            if let Some(duplicate) = item
                .duplicates
                .iter()
                .find(|duplicate| duplicate.path.as_path() == path)
            {
                return duplicate;
            }
        }
        item.duplicates
            .iter()
            .find(|duplicate| duplicate.path == item.default_skill.path)
            .unwrap_or(&item.duplicates[0])
    }

    fn source_content_differs(
        duplicate: &SkillInventoryDuplicate,
        default_skill: &SkillInventoryDuplicate,
    ) -> bool {
        duplicate.path != default_skill.path && duplicate.content != default_skill.content
    }

    fn provider_pip_color(provider: SkillProvider, theme: &WarpTheme) -> ColorU {
        match provider {
            SkillProvider::Agents | SkillProvider::Github => {
                theme.main_text_color(theme.background()).into_solid()
            }
            SkillProvider::Zap => theme.accent().into_solid(),
            SkillProvider::Claude => CLAUDE_ORANGE,
            SkillProvider::Codex => theme.ansi_fg_green(),
            SkillProvider::Cursor => theme.ansi_fg_cyan(),
            SkillProvider::Gemini => theme.ansi_fg_blue(),
            SkillProvider::Copilot => theme.ansi_fg_magenta(),
            SkillProvider::Droid => theme.ansi_fg_yellow(),
            SkillProvider::OpenCode => theme.ansi_fg_red(),
        }
    }

    fn skill_row_position_id(name: &str) -> String {
        format!("skill-manager-row:{name}")
    }

    fn scroll_selected_path_into_view(&self, items: &[SkillInventoryItem]) {
        let Some(path) = self.selected_path.as_deref() else {
            return;
        };
        let Some(item) = items.iter().find(|item| {
            item.duplicates
                .iter()
                .any(|duplicate| duplicate.path.as_path() == path)
        }) else {
            return;
        };

        self.list_scroll_state.scroll_to_position(ScrollTarget {
            position_id: Self::skill_row_position_id(&item.name),
            mode: ScrollToPositionMode::FullyIntoView,
        });
    }

    fn scroll_selected_path_into_view_with_ctx(&self, ctx: &AppContext) {
        let inventory = SkillManager::as_ref(ctx).list_skill_inventory(ctx);
        let query = self.query(ctx);
        let items = Self::filter_inventory(&inventory, &query, self.provider_filter);
        self.scroll_selected_path_into_view(&items);
    }

    fn filter_mouse_state_for(&self, provider: Option<SkillProvider>) -> MouseStateHandle {
        self.filter_mouse_states
            .borrow_mut()
            .entry(provider)
            .or_default()
            .clone()
    }

    fn row_mouse_state_for(&self, path: &Path) -> MouseStateHandle {
        self.row_mouse_states
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_default()
            .clone()
    }

    fn tag_mouse_state_for(&self, path: &Path) -> MouseStateHandle {
        self.tag_mouse_states
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_default()
            .clone()
    }

    fn hovered_tag_path(&self, item: &SkillInventoryItem) -> Option<PathBuf> {
        for duplicate in &item.duplicates {
            let hovered = self
                .tag_mouse_states
                .borrow()
                .get(&duplicate.path)
                .and_then(|state| state.lock().ok().map(|mouse| mouse.is_hovered()))
                .unwrap_or(false);
            if hovered {
                return Some(duplicate.path.clone());
            }
        }
        None
    }

    fn render_dot(color: ColorU, diameter: f32) -> Box<dyn Element> {
        Container::new(
            ConstrainedBox::new(Empty::new().finish())
                .with_width(diameter)
                .with_height(diameter)
                .finish(),
        )
        .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
        .with_background_color(color)
        .finish()
    }

    fn with_diff_dot(tag: Box<dyn Element>, color: ColorU) -> Box<dyn Element> {
        Stack::new()
            .with_child(tag)
            .with_positioned_child(
                Self::render_dot(color, SOURCE_TAG_PIP),
                OffsetPositioning::offset_from_parent(
                    vec2f(SOURCE_TAG_PIP / 2.0, -(SOURCE_TAG_PIP / 2.0)),
                    ParentOffsetBounds::Unbounded,
                    ParentAnchor::TopRight,
                    ChildAnchor::TopRight,
                ),
            )
            .finish()
    }

    fn render_label(
        text: impl Into<String>,
        appearance: &Appearance,
        font_size: f32,
        color: impl Into<pathfinder_color::ColorU>,
    ) -> Box<dyn Element> {
        Text::new_inline(text.into(), appearance.ui_font_family(), font_size)
            .with_color(color.into())
            .with_clip(ClipConfig::ellipsis())
            .finish()
    }

    fn render_filter_button(
        &self,
        label: String,
        is_active: bool,
        provider: Option<SkillProvider>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = if is_active {
            theme.main_text_color(theme.background())
        } else {
            theme.sub_text_color(theme.background())
        };
        let state = self.filter_mouse_state_for(provider);
        let action = SkillManagerPanelAction::SelectProviderFilter(provider);

        Hoverable::new(state, move |mouse| {
            // 文字用 Flex::row + CrossAxisAlignment::Center 垂直居中于固定行高里。
            let label_row = Flex::row()
                .with_main_axis_size(MainAxisSize::Min)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Self::render_label(
                    label.clone(),
                    appearance,
                    appearance.ui_font_body(),
                    text_color,
                ))
                .finish();
            let mut button = Container::new(label_row)
                .with_padding_left(FILTER_BUTTON_HORIZONTAL_PADDING)
                .with_padding_right(FILTER_BUTTON_HORIZONTAL_PADDING)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                    FILTER_BUTTON_CORNER_RADIUS,
                )));
            if is_active {
                // 激活态使用 fg_overlay_4 让对比更强,避免快速切换时看不出选中变化。
                button = button.with_background(internal_colors::fg_overlay_4(theme));
            } else if mouse.is_hovered() {
                button = button.with_background(internal_colors::fg_overlay_2(theme));
            }
            ConstrainedBox::new(button.finish())
                .with_height(FILTER_BUTTON_HEIGHT)
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_mouse_down(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish()
    }

    fn render_filter_rows(
        &self,
        providers: &[SkillProvider],
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        // 来源过多时折叠为「全部 + 来源下拉」,避免窄侧栏放不下被裁断。
        if providers.len() > FILTER_TABS_INLINE_MAX {
            return self.render_collapsed_filter_rows(appearance);
        }

        let active_filter = self.provider_filter;

        // 使用 MainAxisSize::Max 让过滤标签行填满面板宽度,消除右侧留白。
        let mut filter_buttons = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(4.0)
            .with_child(self.render_filter_button(
                crate::t!("skill-manager-filter-all").into(),
                active_filter.is_none(),
                None,
                appearance,
            ));

        for provider in providers {
            filter_buttons.add_child(self.render_filter_button(
                provider.to_string(),
                active_filter == Some(*provider),
                Some(*provider),
                appearance,
            ));
        }

        // 不再外套 ClippedScrollable / 外壳容器:
        // 1. 横向滚动容器会拦截 mouse-down 用于拖拽判定,导致快速切换 tab 时点击响应延迟。
        // 2. 平铺态 provider 数 ≤ FILTER_TABS_INLINE_MAX,侧栏宽度可容纳;
        //    更多来源走折叠下拉分支。
        Clipped::new(filter_buttons.finish()).finish()
    }

    /// 折叠态过滤行:左侧「全部」快捷按钮 + 右侧「来源 ▾」下拉。
    ///
    /// 绝不外套 `Clipped`:下拉展开的菜单是定位 overlay,被裁剪会丢失。
    fn render_collapsed_filter_rows(&self, appearance: &Appearance) -> Box<dyn Element> {
        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(4.0)
            .with_child(self.render_filter_button(
                crate::t!("skill-manager-filter-all").into(),
                self.provider_filter.is_none(),
                None,
                appearance,
            ))
            .with_child(ChildView::new(&self.source_dropdown).finish())
            .finish()
    }

    fn render_search_input(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let search_row = Shrinkable::new(
            1.0,
            Clipped::new(ChildView::new(&self.query_editor).finish()).finish(),
        )
        .finish();

        Container::new(search_row)
            .with_padding(Padding::uniform(6.0).with_left(12.0).with_right(12.0))
            .with_border(Border::all(1.0).with_border_fill(theme.surface_3()))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
            .finish()
    }

    fn render_source_tag(
        &self,
        item: &SkillInventoryItem,
        duplicate: &SkillInventoryDuplicate,
        pinned_path: &Path,
        any_tag_hovered: bool,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let tag_path = duplicate.path.clone();
        let pinned_path = pinned_path.to_path_buf();
        let skill_name = item.name.clone();
        let label = duplicate.provider.to_string();
        let is_effective = duplicate.path == item.default_skill.path;
        let content_differs = Self::source_content_differs(duplicate, &item.default_skill);
        let pip_color = Self::provider_pip_color(duplicate.provider, theme);
        let diff_color = theme.ui_yellow_color();
        let state = self.tag_mouse_state_for(&duplicate.path);

        let tag = Hoverable::new(state, move |mouse| {
            let is_shown = if any_tag_hovered {
                mouse.is_hovered()
            } else {
                tag_path == pinned_path
            };
            let text_color = if is_shown {
                theme.main_text_color(theme.background())
            } else {
                theme.sub_text_color(theme.background())
            };
            let mut row = Flex::row()
                .with_main_axis_size(MainAxisSize::Min)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(4.0)
                .with_child(Self::render_dot(pip_color, SOURCE_TAG_PIP))
                .with_child(Self::render_label(
                    label.clone(),
                    appearance,
                    appearance.ui_font_footnote(),
                    text_color,
                ));
            if is_effective {
                row.add_child(
                    ConstrainedBox::new(WarpIcon::Check.to_warpui_icon(text_color).finish())
                        .with_width(SOURCE_TAG_CHECK)
                        .with_height(SOURCE_TAG_CHECK)
                        .finish(),
                );
            }
            let mut tag = Container::new(row.finish())
                .with_padding_left(6.0)
                .with_padding_right(6.0)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)));
            if is_shown {
                tag = tag.with_background(internal_colors::fg_overlay_3(theme));
            } else {
                tag = tag.with_border(Border::all(1.0).with_border_fill(theme.surface_3()));
            }
            let tag = ConstrainedBox::new(tag.finish())
                .with_height(SOURCE_TAG_HEIGHT)
                .finish();
            if content_differs {
                Self::with_diff_dot(tag, diff_color)
            } else {
                tag
            }
        })
        .with_cursor(Cursor::PointingHand)
        .on_click({
            let name = skill_name.clone();
            let path = duplicate.path.clone();
            move |ctx, _, _| {
                ctx.dispatch_typed_action(SkillManagerPanelAction::PinSkillSource {
                    name: name.clone(),
                    path: path.clone(),
                    open: false,
                });
            }
        })
        .on_double_click({
            let path = duplicate.path.clone();
            move |ctx, _, _| {
                ctx.dispatch_typed_action(SkillManagerPanelAction::PinSkillSource {
                    name: skill_name.clone(),
                    path: path.clone(),
                    open: true,
                });
            }
        })
        .finish();
        tag
    }

    fn render_skill_row(
        &self,
        item: &SkillInventoryItem,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let pinned = Self::resolve_shown_duplicate(
            item,
            self.pinned_paths.get(&item.name).map(PathBuf::as_path),
            None,
        );
        let hovered_path = self.hovered_tag_path(item);
        let shown = Self::resolve_shown_duplicate(
            item,
            Some(pinned.path.as_path()),
            hovered_path.as_deref(),
        );
        let is_selected = self.selected_path.as_deref().is_some_and(|path| {
            item.duplicates
                .iter()
                .any(|duplicate| duplicate.path.as_path() == path)
        });
        let description_color = if shown.description != item.default_skill.description {
            theme.ui_yellow_color()
        } else {
            theme.sub_text_color(theme.background()).into_solid()
        };

        let title = Self::render_label(
            item.name.clone(),
            appearance,
            appearance.ui_font_body_large(),
            theme.main_text_color(theme.background()),
        );
        let description = Self::render_label(
            shown.description.clone(),
            appearance,
            appearance.ui_font_footnote(),
            description_color,
        );
        let path_label = Self::render_label(
            shown.path.display().to_string(),
            appearance,
            appearance.ui_font_footnote(),
            theme.sub_text_color(theme.background()),
        );

        let mut column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(2.0)
            .with_child(title)
            .with_child(description);
        if item.has_duplicates() {
            let mut tags = Wrap::row()
                .with_spacing(4.0)
                .with_run_spacing(4.0)
                .with_cross_axis_alignment(CrossAxisAlignment::Center);
            for duplicate in &item.duplicates {
                tags.extend(std::iter::once(self.render_source_tag(
                    item,
                    duplicate,
                    pinned.path.as_path(),
                    hovered_path.is_some(),
                    appearance,
                )));
            }
            column.add_child(
                Container::new(tags.finish())
                    .with_padding_top(4.0)
                    .with_padding_bottom(2.0)
                    .finish(),
            );
        }
        column.add_child(path_label);

        let action = SkillManagerPanelAction::EditSkill(pinned.path.clone());
        let position_id = Self::skill_row_position_id(&item.name);
        let state = self.row_mouse_state_for(&item.default_skill.path);
        // 标签自己处理点击. 行的按下只在点到标签以外时打开当前记住的那份.
        let row = Hoverable::new(state, move |mouse| {
            let background = if is_selected && mouse.is_hovered() {
                Some(internal_colors::fg_overlay_4(theme))
            } else if is_selected {
                Some(internal_colors::fg_overlay_3(theme))
            } else if mouse.is_hovered() {
                Some(internal_colors::fg_overlay_2(theme))
            } else {
                None
            };
            // 外层 Flex::row 使用 MainAxisSize::Max 填满面板宽度,
            // 避免 Container 收缩到内容自然宽度导致高亮区域右侧留白。
            let content = Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_child(column.finish())
                .finish();
            let mut row = Container::new(content)
                .with_padding_top(ROW_PADDING_VERTICAL)
                .with_padding_bottom(ROW_PADDING_VERTICAL)
                .with_padding_left(ROW_PADDING_HORIZONTAL)
                .with_padding_right(ROW_PADDING_HORIZONTAL)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)));
            if let Some(background) = background {
                row = row.with_background(background);
            }
            row.finish()
        })
        .with_defer_events_to_children()
        .with_cursor(Cursor::PointingHand)
        .on_mouse_down(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish();

        SavePosition::new(row, &position_id).finish()
    }

    fn render_skill_list(
        &self,
        items: &[SkillInventoryItem],
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        if items.is_empty() {
            return Container::new(Self::render_label(
                crate::t!("skill-manager-empty"),
                appearance,
                appearance.ui_font_body_large(),
                appearance
                    .theme()
                    .sub_text_color(appearance.theme().background()),
            ))
            .with_uniform_padding(12.0)
            .finish();
        }

        let mut rows = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(2.0);
        for item in items {
            rows.add_child(self.render_skill_row(item, appearance));
        }

        let theme = appearance.theme();
        ClippedScrollable::vertical(
            self.list_scroll_state.clone(),
            rows.finish(),
            ScrollbarWidth::Auto,
            theme.disabled_text_color(theme.background()).into(),
            theme.main_text_color(theme.background()).into(),
            ElementFill::None,
        )
        .with_overlayed_scrollbar()
        .finish()
    }
}

impl TypedActionView for SkillManagerPanel {
    type Action = SkillManagerPanelAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SkillManagerPanelAction::SelectProviderFilter(provider) => {
                if self.provider_filter == *provider {
                    return;
                }
                self.provider_filter = *provider;
                // 同步折叠态下拉的选中项:点击左侧「全部」按钮时下拉并不知情。
                self.rebuild_source_dropdown(ctx);
                ctx.notify();
            }
            SkillManagerPanelAction::EditSkill(path) => {
                self.open_skill_file(path.clone(), ctx);
                ctx.notify();
            }
            SkillManagerPanelAction::PinSkillSource { name, path, open } => {
                self.pinned_paths.insert(name.clone(), path.clone());
                if *open {
                    self.open_skill_file(path.clone(), ctx);
                }
                ctx.notify();
            }
        }
    }
}

impl SkillManagerPanel {
    fn open_skill_file(&mut self, path: PathBuf, ctx: &mut ViewContext<Self>) {
        self.selected_path = Some(path.clone());
        self.scroll_selected_path_into_view_with_ctx(ctx);
        ctx.emit(SkillManagerPanelEvent::OpenSkillFile { path });
    }
}

impl View for SkillManagerPanel {
    fn ui_name() -> &'static str {
        "SkillManagerPanel"
    }

    fn on_focus(&mut self, _focus_ctx: &warpui::FocusContext, ctx: &mut ViewContext<Self>) {
        ctx.focus(&self.query_editor);
        self.scroll_selected_path_into_view_with_ctx(ctx);
        ctx.notify();
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let inventory = SkillManager::as_ref(app).list_skill_inventory(app);
        let providers = Self::providers_in_inventory(&inventory);
        let query = self.query(app);
        let items = Self::filter_inventory(&inventory, &query, self.provider_filter);

        Container::new(
            Flex::column()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_spacing(8.0)
                .with_child(self.render_search_input(appearance))
                .with_child(self.render_filter_rows(&providers, appearance))
                .with_child(
                    Shrinkable::new(1.0, self.render_skill_list(&items, appearance)).finish(),
                )
                .finish(),
        )
        .with_uniform_padding(PANEL_PADDING)
        .finish()
    }
}

impl Entity for SkillManagerPanel {
    type Event = SkillManagerPanelEvent;
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
