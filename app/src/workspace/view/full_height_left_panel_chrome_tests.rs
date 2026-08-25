use super::{
    header_items_excluding_lifted_tools_panel, left_panel_titlebar_leading_inset,
    tab_bar_leading_padding, use_full_height_left_panel_chrome,
};
use crate::workspace::header_toolbar_item::HeaderToolbarItemKind;

#[test]
fn use_full_height_left_panel_chrome_truth_table() {
    assert!(use_full_height_left_panel_chrome(
        true, true, false, false, false
    ));
    assert!(!use_full_height_left_panel_chrome(
        false, true, false, false, false
    ));
    assert!(!use_full_height_left_panel_chrome(
        true, false, false, false, false
    ));
    assert!(!use_full_height_left_panel_chrome(
        true, true, true, false, false
    ));
    assert!(!use_full_height_left_panel_chrome(
        true, true, false, true, false
    ));
    assert!(!use_full_height_left_panel_chrome(
        true, true, false, false, true
    ));
}

#[test]
fn header_items_excluding_lifted_tools_panel_drops_tools_panel_only_when_chrome_is_on() {
    let items = vec![
        HeaderToolbarItemKind::TabsPanel,
        HeaderToolbarItemKind::ToolsPanel,
        HeaderToolbarItemKind::CodeReview,
    ];

    assert_eq!(
        header_items_excluding_lifted_tools_panel(items.clone(), true),
        vec![
            HeaderToolbarItemKind::TabsPanel,
            HeaderToolbarItemKind::CodeReview,
        ]
    );
    assert_eq!(
        header_items_excluding_lifted_tools_panel(items, false),
        vec![
            HeaderToolbarItemKind::TabsPanel,
            HeaderToolbarItemKind::ToolsPanel,
            HeaderToolbarItemKind::CodeReview,
        ]
    );
}

#[test]
fn tab_bar_leading_padding_omits_traffic_lights_when_chrome_is_on() {
    let traffic_light_width = 64.;
    assert_eq!(
        tab_bar_leading_padding(true, false, false, traffic_light_width),
        super::super::TAB_BAR_PADDING_LEFT
    );
    assert_eq!(
        tab_bar_leading_padding(false, false, false, traffic_light_width),
        traffic_light_width + 16.
    );
    assert_eq!(
        tab_bar_leading_padding(false, true, false, traffic_light_width),
        0.
    );
    assert_eq!(
        tab_bar_leading_padding(false, false, true, traffic_light_width),
        super::super::TAB_BAR_PADDING_LEFT
    );
}

#[test]
fn left_panel_titlebar_leading_inset_takes_macos_traffic_lights_when_chrome_is_on() {
    assert_eq!(
        left_panel_titlebar_leading_inset(true, false, 64.),
        64.
    );
    assert_eq!(
        left_panel_titlebar_leading_inset(true, true, 64.),
        0.
    );
    assert_eq!(
        left_panel_titlebar_leading_inset(false, false, 64.),
        0.
    );
    assert_eq!(
        left_panel_titlebar_leading_inset(true, false, 0.),
        0.
    );
}
