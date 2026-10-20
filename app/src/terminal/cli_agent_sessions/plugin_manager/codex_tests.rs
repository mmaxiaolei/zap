use super::{
    codex_hook_trust_hash, install_codex_hooks, merge_tui_notification_settings, CodexPluginManager,
};
use crate::terminal::cli_agent_sessions::plugin_manager::CliAgentPluginManager;

#[test]
fn can_auto_install_is_true() {
    assert!(CodexPluginManager.can_auto_install());
}

#[test]
fn supports_update_is_true() {
    assert!(CodexPluginManager.supports_update());
}

#[test]
fn install_instructions_has_steps() {
    let instructions = CodexPluginManager.install_instructions();
    assert!(!instructions.steps.is_empty());
    assert!(!instructions.title.is_empty());
}

#[test]
fn trust_hash_matches_known_codex_algorithm() {
    let hash = codex_hook_trust_hash(
        "post_tool_use",
        Some("Edit|Write"),
        "/Users/admin/.claude/bin/claude-stats xrecord",
        600,
        None,
    );
    assert_eq!(
        hash,
        "sha256:fe1b63d260d2ebcd42bbf50d9cc5c989b937bbe5792fdfc2473bf6c1e0b1d00f"
    );
}

#[test]
fn bundled_hooks_json_mentions_codex_scripts() {
    let json = include_str!("../../../../assets/bundled/cli_agent_hooks/codex/zap-cli-agent.json");
    assert!(json.contains("UserPromptSubmit"));
    assert!(json.contains("Stop"));
    assert!(json.contains("zap-cli-agent/scripts/on-stop.sh"));
}

#[test]
fn install_codex_hooks_is_idempotent() {
    let _ = install_codex_hooks();
    let again = install_codex_hooks();
    assert!(
        again.is_ok(),
        "reinstall failed: {}",
        again.err().map(|e| e.message).unwrap_or_default()
    );
    assert!(CodexPluginManager.is_installed());
    let home = dirs::home_dir().unwrap();
    let cfg = std::fs::read_to_string(home.join(".codex").join("config.toml")).unwrap_or_default();
    assert_eq!(
        cfg.lines().filter(|l| l.trim() == "[tui]").count(),
        1,
        "duplicate [tui] after install"
    );

    // Regression: Zap UserPromptSubmit trust must match the live hooks.json index
    // (index drift left PromptSubmit untrusted → sidebar stayed Idle mid-turn).
    let hooks: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".codex").join("hooks.json")).unwrap(),
    )
    .unwrap();
    let groups = hooks["hooks"]["UserPromptSubmit"].as_array().unwrap();
    let mut zap_idx = None;
    let mut zap_cmd = None;
    let mut zap_timeout = 10u64;
    for (gi, group) in groups.iter().enumerate() {
        let Some(handlers) = group.get("hooks").and_then(|h| h.as_array()) else {
            continue;
        };
        for (hi, handler) in handlers.iter().enumerate() {
            let cmd = handler.get("command").and_then(|c| c.as_str()).unwrap_or("");
            if cmd.contains("zap-cli-agent") && cmd.contains("on-prompt-submit") {
                zap_idx = Some((gi, hi));
                zap_cmd = Some(cmd.to_owned());
                zap_timeout = handler
                    .get("timeout")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(10);
            }
        }
    }
    let (gi, hi) = zap_idx.expect("Zap UserPromptSubmit hook missing from hooks.json");
    let cmd = zap_cmd.unwrap();
    let expected = codex_hook_trust_hash("user_prompt_submit", None, &cmd, zap_timeout, None);
    let key = format!(
        "{}:user_prompt_submit:{gi}:{hi}",
        home.join(".codex").join("hooks.json").display()
    );
    let section = format!("[hooks.state.\"{key}\"]");
    assert!(
        cfg.contains(&section),
        "missing trust section for live Zap UPS index {gi}:{hi}; section={section}; cfg_has_1_0={}; sample={}",
        cfg.contains("user_prompt_submit:1:0"),
        cfg.lines()
            .filter(|l| l.contains("user_prompt_submit"))
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let start = cfg.find(&section).unwrap();
    let rest = &cfg[start..];
    let end = rest.find("\n[").unwrap_or(rest.len().min(240));
    let block = &rest[..end];
    assert!(
        block.contains(&expected),
        "trust hash mismatch for {key}: expected {expected} in {block}"
    );
    assert_eq!(
        cfg.matches(expected.as_str()).count(),
        1,
        "Zap UPS hash should appear exactly once (no stale index)"
    );
}

#[test]
fn merge_tui_notification_settings_into_existing() {
    let before = concat!(
        "[tui]\n",
        "  screen_reader_detection_done = true\n",
        "\n",
        "[tui.model_availability_nux]\n",
        "\"gpt-5.5\" = 4\n",
    );
    let after = merge_tui_notification_settings(before);
    assert_eq!(after.matches("[tui]").count(), 1);
    assert!(after.contains("notification_condition = \"always\""));
    assert!(after.contains("notification_method = \"osc9\""));
    assert!(after.contains("screen_reader_detection_done = true"));
    assert!(after.contains("[tui.model_availability_nux]"));
}

#[test]
fn merge_tui_notification_settings_idempotent() {
    let once = merge_tui_notification_settings("[features]\nhooks = true\n");
    let twice = merge_tui_notification_settings(&once);
    assert_eq!(once, twice);
    assert_eq!(once.matches("[tui]").count(), 1);
}

#[test]
fn merge_tui_does_not_duplicate_when_always_present() {
    let before =
        "[tui]\n  notification_condition = \"always\"\n  screen_reader_detection_done = true\n";
    let after = merge_tui_notification_settings(before);
    assert_eq!(before, after);
}


#[test]
fn strip_stale_zap_trust_removes_wrong_index_only() {
    use super::strip_stale_zap_hooks_json_trust;
    use std::collections::BTreeMap;

    let zap_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let orca_hash = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let cfg = format!(
        concat!(
            "[hooks.state.\"/Users/admin/.codex/hooks.json:user_prompt_submit:0:0\"]\n",
            "  enabled = true\n",
            "  trusted_hash = \"{orca}\"\n",
            "\n",
            "[hooks.state.\"/Users/admin/.codex/hooks.json:user_prompt_submit:2:0\"]\n",
            "  enabled = true\n",
            "  trusted_hash = \"{zap}\"\n",
            "\n",
            "[hooks.state.\"/Users/admin/.codex/config.toml:stop:0:0\"]\n",
            "  enabled = true\n",
            "  trusted_hash = \"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"\n",
        ),
        orca = orca_hash,
        zap = zap_hash,
    );
    let mut keep = BTreeMap::new();
    keep.insert(
        "/Users/admin/.codex/hooks.json:user_prompt_submit:1:0".to_owned(),
        zap_hash.to_owned(),
    );
    let out = strip_stale_zap_hooks_json_trust(&cfg, &keep);
    assert!(out.contains("user_prompt_submit:0:0"), "orca trust kept");
    assert!(
        !out.contains("user_prompt_submit:2:0"),
        "stale zap index removed"
    );
    assert!(out.contains("config.toml:stop:0:0"), "toml trust kept");
}
