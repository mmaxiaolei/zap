use crate::workspace::header_toolbar_item::HeaderToolbarItemKind;

use super::TAB_BAR_PADDING_LEFT;

/// 项目组织模式下，侧栏是否通顶、TabBar 是否只出现在内容列。
pub(crate) fn use_full_height_left_panel_chrome(
    repository_workspaces_enabled: bool,
    left_panel_open: bool,
    simplified_wasm_tab_bar: bool,
    vertical_tabs_active: bool,
    mobile_overlay: bool,
) -> bool {
    repository_workspaces_enabled
        && left_panel_open
        && !simplified_wasm_tab_bar
        && !vertical_tabs_active
        && !mobile_overlay
}

/// 新 chrome 已把 ToolsPanel 提到窗口左侧时，从 header toolbar 配置中去掉它，避免画两次。
pub(crate) fn header_items_excluding_lifted_tools_panel(
    items: impl IntoIterator<Item = HeaderToolbarItemKind>,
    full_height_chrome: bool,
) -> Vec<HeaderToolbarItemKind> {
    items
        .into_iter()
        .filter(|item| !(full_height_chrome && *item == HeaderToolbarItemKind::ToolsPanel))
        .collect()
}

/// TabBar 左侧 padding。新 chrome 下红绿灯改由侧栏头承担。
pub(crate) fn tab_bar_leading_padding(
    full_height_chrome: bool,
    theme_chooser_open: bool,
    is_macos_fullscreen: bool,
    left_traffic_light_width: f32,
) -> f32 {
    if theme_chooser_open {
        0.
    } else if full_height_chrome || is_macos_fullscreen {
        TAB_BAR_PADDING_LEFT
    } else {
        left_traffic_light_width + 16.
    }
}

/// 侧栏工具条头为 macOS 窗口左上红绿灯预留的宽度。Windows/Linux 传入 0。
pub(crate) fn left_panel_titlebar_leading_inset(
    full_height_chrome: bool,
    is_macos_fullscreen: bool,
    left_traffic_light_width: f32,
) -> f32 {
    if full_height_chrome && !is_macos_fullscreen {
        left_traffic_light_width
    } else {
        0.
    }
}

#[cfg(test)]
#[path = "full_height_left_panel_chrome_tests.rs"]
mod tests;
