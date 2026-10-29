use super::ChannelState;

// Zap Wave 5-5：`derive_http_origin_from_ws_url` 调用 + 3 个 wss/ws 路径测试随
// `ChannelState::rtc_http_url()` 一同物理删。

/// `ChannelState::init()` (the static default for OSS builds) must satisfy
/// the cloud-disabled predicate; the cloud-removal plan's Phase 5 short-circuit
/// depends on this invariant.
#[test]
fn default_oss_state_is_cloud_disabled() {
    assert!(ChannelState::is_cloud_disabled());
}

#[test]
fn unpackaged_oss_skips_session_restore() {
    use super::Channel;
    assert!(ChannelState::should_skip_session_restore_for(Channel::Oss, false));
}

#[test]
fn packaged_oss_restores_session() {
    use super::Channel;
    // release_bundle and/or running inside Zap.app both pass is_packaged=true.
    assert!(!ChannelState::should_skip_session_restore_for(Channel::Oss, true));
}

#[test]
fn non_oss_channels_do_not_skip_session_restore() {
    use super::Channel;
    for channel in [
        Channel::Stable,
        Channel::Preview,
        Channel::Dev,
        Channel::Local,
        Channel::Integration,
    ] {
        assert!(
            !ChannelState::should_skip_session_restore_for(channel, false),
            "{channel:?} should not skip solely because unpackaged"
        );
        assert!(!ChannelState::should_skip_session_restore_for(channel, true));
    }
}

#[test]
fn macos_app_bundle_path_detection_helper_is_safe_off_macos_path() {
    // Pure helper: when current_exe is not under .app/Contents/MacOS, returns false
    // unless release_bundle is compiled in. Just smoke that the API exists.
    let _ = ChannelState::is_running_inside_macos_app_bundle();
    let _ = ChannelState::is_packaged_for_session_persistence();
}
