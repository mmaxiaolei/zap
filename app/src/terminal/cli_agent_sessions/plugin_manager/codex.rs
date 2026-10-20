//! Codex CLI hooks for Zap sidebar status.
//!
//! Codex's built-in OSC 9 notifications only fire when unfocused by default, so
//! the sidebar stayed Idle after turns. We install UserPromptSubmit / Stop /
//! SessionStart / PermissionRequest hooks under `~/.codex/hooks/zap-cli-agent/`
//! (merged into `~/.codex/hooks.json`) that emit OSC 777 `warp://cli-agent`
//! events — same protocol as Claude/Grok — and auto-trust them in config.toml.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use super::{CliAgentPluginManager, PluginInstallError, PluginInstructionStep, PluginInstructions};

const PLUGIN_VERSION: &str = "1.0.0";
const ZAP_MARKER: &str = "zap-cli-agent/scripts/";

const HOOKS_JSON: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/codex/zap-cli-agent.json");
const SCRIPT_BUILD_PAYLOAD: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/codex/scripts/build-payload.sh");
const SCRIPT_ZAP_NOTIFY: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/codex/scripts/zap-notify.sh");
const SCRIPT_ON_STOP: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/codex/scripts/on-stop.sh");
const SCRIPT_ON_PROMPT: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/codex/scripts/on-prompt-submit.sh");
const SCRIPT_ON_SESSION: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/codex/scripts/on-session-start.sh");
const SCRIPT_ON_PERM: &str = include_str!(
    "../../../../assets/bundled/cli_agent_hooks/codex/scripts/on-permission-request.sh"
);

pub(super) struct CodexPluginManager;

fn codex_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
}

fn scripts_dir() -> PathBuf {
    codex_home().join("hooks").join("zap-cli-agent").join("scripts")
}

fn hooks_json_path() -> PathBuf {
    codex_home().join("hooks.json")
}

fn config_toml_path() -> PathBuf {
    codex_home().join("config.toml")
}

fn write_executable(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn materialize_home(s: &str) -> String {
    let home = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .into_owned();
    s.replace("$HOME", &home)
}

/// SHA-256 identity hash matching Codex `version_for_toml` / `hook_hash`.
fn codex_hook_trust_hash(
    event_snake: &str,
    matcher: Option<&str>,
    command: &str,
    timeout: u64,
    status_message: Option<&str>,
) -> String {
    let mut handler = json!({
        "type": "command",
        "command": command,
        "timeout": timeout,
        "async": false,
    });
    if let Some(msg) = status_message {
        handler
            .as_object_mut()
            .unwrap()
            .insert("statusMessage".to_owned(), JsonValue::String(msg.to_owned()));
    }
    let mut identity = json!({
        "event_name": event_snake,
        "hooks": [handler],
    });
    if let Some(m) = matcher {
        identity
            .as_object_mut()
            .unwrap()
            .insert("matcher".to_owned(), JsonValue::String(m.to_owned()));
    } else {
        identity
            .as_object_mut()
            .unwrap()
            .insert("matcher".to_owned(), JsonValue::Null);
    }
    sort_json_object_keys(&mut identity);
    let serialized = serde_json::to_vec(&identity).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&serialized);
    format!("sha256:{:x}", hasher.finalize())
}

fn sort_json_object_keys(value: &mut JsonValue) {
    match value {
        JsonValue::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            let mut sorted = serde_json::Map::new();
            let mut keys = keys;
            keys.sort();
            for k in keys {
                if let Some(mut v) = map.remove(&k) {
                    sort_json_object_keys(&mut v);
                    sorted.insert(k, v);
                }
            }
            *map = sorted;
        }
        JsonValue::Array(arr) => {
            for item in arr {
                sort_json_object_keys(item);
            }
        }
        _ => {}
    }
}

fn event_snake(event_pascal: &str) -> &'static str {
    match event_pascal {
        "SessionStart" => "session_start",
        "UserPromptSubmit" => "user_prompt_submit",
        "Stop" => "stop",
        "PermissionRequest" => "permission_request",
        _ => "unknown",
    }
}

/// Install scripts, merge Zap hooks into `~/.codex/hooks.json`, trust them, and
/// enable always-on TUI OSC 9 as a Stop fallback.
pub(crate) fn install_codex_hooks() -> Result<(), PluginInstallError> {
    let mut log = String::new();
    let scripts = scripts_dir();
    let files: &[(&str, &str)] = &[
        ("build-payload.sh", SCRIPT_BUILD_PAYLOAD),
        ("zap-notify.sh", SCRIPT_ZAP_NOTIFY),
        ("on-stop.sh", SCRIPT_ON_STOP),
        ("on-prompt-submit.sh", SCRIPT_ON_PROMPT),
        ("on-session-start.sh", SCRIPT_ON_SESSION),
        ("on-permission-request.sh", SCRIPT_ON_PERM),
    ];
    for (name, body) in files {
        let path = scripts.join(name);
        write_executable(&path, body).map_err(|err| PluginInstallError {
            message: format!("failed to write {}", path.display()),
            log: {
                log.push_str(&format!("error writing {}: {err}\n", path.display()));
                log.clone()
            },
        })?;
        log.push_str(&format!("wrote {}\n", path.display()));
    }

    merge_hooks_json(&mut log)?;
    write_trust_state(&mut log)?;
    ensure_tui_notifications(&mut log)?;
    let _ = log;
    Ok(())
}

fn zap_hook_fragment() -> Result<JsonValue, PluginInstallError> {
    let raw = materialize_home(HOOKS_JSON);
    serde_json::from_str(&raw).map_err(|err| PluginInstallError {
        message: format!("invalid bundled Codex hooks JSON: {err}"),
        log: err.to_string(),
    })
}

fn merge_hooks_json(log: &mut String) -> Result<(), PluginInstallError> {
    let path = hooks_json_path();
    let mut root = if path.is_file() {
        let contents = fs::read_to_string(&path).map_err(|err| PluginInstallError {
            message: format!("failed to read {}", path.display()),
            log: err.to_string(),
        })?;
        serde_json::from_str::<JsonValue>(&contents).unwrap_or_else(|_| json!({"hooks": {}}))
    } else {
        json!({"hooks": {}})
    };

    let hooks = root
        .as_object_mut()
        .ok_or_else(|| PluginInstallError {
            message: "hooks.json root must be an object".to_owned(),
            log: String::new(),
        })?
        .entry("hooks")
        .or_insert_with(|| json!({}));
    let hooks_obj = hooks.as_object_mut().ok_or_else(|| PluginInstallError {
        message: "hooks.json hooks must be an object".to_owned(),
        log: String::new(),
    })?;

    let fragment = zap_hook_fragment()?;
    let frag_hooks = fragment
        .get("hooks")
        .and_then(|h| h.as_object())
        .ok_or_else(|| PluginInstallError {
            message: "bundled fragment missing hooks".to_owned(),
            log: String::new(),
        })?;

    for (event, groups) in frag_hooks {
        let existing = hooks_obj
            .entry(event.clone())
            .or_insert_with(|| json!([]));
        let arr = existing.as_array_mut().ok_or_else(|| PluginInstallError {
            message: format!("hooks.{event} must be an array"),
            log: String::new(),
        })?;
        // Drop prior Zap groups (idempotent reinstall).
        arr.retain(|group| {
            !serde_json::to_string(group)
                .unwrap_or_default()
                .contains(ZAP_MARKER)
        });
        if let Some(new_groups) = groups.as_array() {
            for g in new_groups {
                arr.push(g.clone());
            }
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| PluginInstallError {
            message: format!("failed to create {}", parent.display()),
            log: err.to_string(),
        })?;
    }
    let pretty = serde_json::to_string_pretty(&root).map_err(|err| PluginInstallError {
        message: format!("failed to serialize hooks.json: {err}"),
        log: err.to_string(),
    })?;
    fs::write(&path, pretty).map_err(|err| PluginInstallError {
        message: format!("failed to write {}", path.display()),
        log: err.to_string(),
    })?;
    log.push_str(&format!("merged Zap hooks into {}\n", path.display()));
    Ok(())
}

fn write_trust_state(log: &mut String) -> Result<(), PluginInstallError> {
    let hooks_path = hooks_json_path();
    let contents = fs::read_to_string(&hooks_path).map_err(|err| PluginInstallError {
        message: format!("failed to read {}", hooks_path.display()),
        log: err.to_string(),
    })?;
    let root: JsonValue = serde_json::from_str(&contents).map_err(|err| PluginInstallError {
        message: format!("failed to parse hooks.json: {err}"),
        log: err.to_string(),
    })?;
    let Some(hooks_obj) = root.get("hooks").and_then(|h| h.as_object()) else {
        return Ok(());
    };

    let key_source = hooks_path.display().to_string();
    let mut trust_entries: BTreeMap<String, String> = BTreeMap::new();

    for (event_pascal, groups_val) in hooks_obj {
        let Some(groups) = groups_val.as_array() else {
            continue;
        };
        let snake = event_snake(event_pascal);
        if snake == "unknown" {
            continue;
        }
        for (group_idx, group) in groups.iter().enumerate() {
            let group_s = serde_json::to_string(group).unwrap_or_default();
            if !group_s.contains(ZAP_MARKER) {
                continue;
            }
            // Codex forces matcher=None for UserPromptSubmit/Stop/Interrupt when hashing.
            let matcher = if matches!(snake, "user_prompt_submit" | "stop" | "interrupt") {
                None
            } else {
                group
                    .get("matcher")
                    .and_then(|m| m.as_str())
                    .map(str::to_owned)
            };
            let Some(handlers) = group.get("hooks").and_then(|h| h.as_array()) else {
                continue;
            };
            for (handler_idx, handler) in handlers.iter().enumerate() {
                let Some(command) = handler.get("command").and_then(|c| c.as_str()) else {
                    continue;
                };
                let timeout = handler
                    .get("timeout")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(600);
                let status_message = handler
                    .get("statusMessage")
                    .or_else(|| handler.get("status_message"))
                    .and_then(|s| s.as_str());
                // Discovery normalizes timeout; hash must include statusMessage when set.
                let hash = codex_hook_trust_hash(
                    snake,
                    matcher.as_deref(),
                    command,
                    timeout,
                    status_message,
                );
                let key = format!("{key_source}:{snake}:{group_idx}:{handler_idx}");
                trust_entries.insert(key, hash);
            }
        }
    }

    if trust_entries.is_empty() {
        log.push_str("no Zap hooks found to trust\n");
        return Ok(());
    }

    let config_path = config_toml_path();
    let mut existing = fs::read_to_string(&config_path).unwrap_or_default();

    // Drop stale hooks.json trust rows for Zap scripts at old indices (index drift
    // after other hooks are added/removed left UserPromptSubmit untrusted → Idle).
    existing = strip_stale_zap_hooks_json_trust(&existing, &trust_entries);

    for (key, hash) in &trust_entries {
        let section = format!("[hooks.state.\"{key}\"]");
        let block = format!("{section}\n  enabled = true\n  trusted_hash = \"{hash}\"\n");
        if let Some(start) = existing.find(&section) {
            let after = &existing[start + section.len()..];
            let end = after
                .find("\n[")
                .map(|i| start + section.len() + i)
                .unwrap_or(existing.len());
            existing.replace_range(start..end, &block);
        } else {
            if !existing.ends_with('\n') && !existing.is_empty() {
                existing.push('\n');
            }
            existing.push_str(&block);
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
        }
        log.push_str(&format!("trusted {key}\n"));
    }

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| PluginInstallError {
            message: format!("failed to create {}", parent.display()),
            log: err.to_string(),
        })?;
    }
    fs::write(&config_path, existing).map_err(|err| PluginInstallError {
        message: format!("failed to write {}", config_path.display()),
        log: err.to_string(),
    })?;
    Ok(())
}

/// Remove `[hooks.state."…/hooks.json:…"]` sections that are not in `keep`
/// but whose trusted_hash or section key is associated with Zap CLI-agent
/// hooks — prevents index-drift leaving PromptSubmit untrusted.
fn strip_stale_zap_hooks_json_trust(
    config: &str,
    keep: &BTreeMap<String, String>,
) -> String {
    let keep_keys: std::collections::HashSet<&str> =
        keep.keys().map(String::as_str).collect();
    let keep_hashes: std::collections::HashSet<&str> =
        keep.values().map(String::as_str).collect();
    let mut out = String::with_capacity(config.len());
    let mut lines = config.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("[hooks.state.\"") && trimmed.contains("hooks.json:") {
            let mut section_lines = vec![line.to_owned()];
            while let Some(next) = lines.peek() {
                let n = next.trim();
                if n.starts_with('[') {
                    break;
                }
                section_lines.push(lines.next().unwrap().to_owned());
            }
            let key = trimmed
                .strip_prefix("[hooks.state.\"")
                .and_then(|s| s.strip_suffix("\"]"))
                .unwrap_or("");
            let body = section_lines.join("\n");
            let hash = body.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("trusted_hash = \"")
                    .and_then(|s| s.strip_suffix('"'))
            });
            // Only drop rows whose hash is one of *our* Zap hashes but whose
            // key is stale (wrong index after other hooks shifted). Never
            // touch unrelated hooks (Orca/Superset/etc.).
            let is_stale_zap = hash.is_some_and(|h| keep_hashes.contains(h))
                && !keep_keys.contains(key);
            if is_stale_zap {
                continue;
            }
            for sl in section_lines {
                out.push_str(&sl);
                out.push('\n');
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !config.is_empty() && !config.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}


fn ensure_tui_notifications(log: &mut String) -> Result<(), PluginInstallError> {
    let config_path = config_toml_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let updated = merge_tui_notification_settings(&existing);
    if updated == existing {
        log.push_str("tui notification settings already present\n");
        return Ok(());
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| PluginInstallError {
            message: format!("failed to create {}", parent.display()),
            log: err.to_string(),
        })?;
    }
    fs::write(&config_path, updated).map_err(|err| PluginInstallError {
        message: format!("failed to write {}", config_path.display()),
        log: err.to_string(),
    })?;
    log.push_str("merged notification_condition=always into [tui]\n");
    Ok(())
}

/// Pure helper: ensure a single `[tui]` table has `notification_condition` /
/// `notification_method`. Never appends a second `[tui]` heading.
fn merge_tui_notification_settings(existing: &str) -> String {
    let wants = [
        ("notification_condition", "\"always\""),
        ("notification_method", "\"osc9\""),
    ];
    // Already configured — leave file alone.
    if existing.contains("notification_condition = \"always\"")
        || existing.contains("notification_condition=\"always\"")
    {
        return existing.to_owned();
    }

    let lines: Vec<&str> = existing.split_inclusive('\n').collect();
    let mut tui_header: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "[tui]" {
            tui_header = Some(i);
            break;
        }
    }

    let mut missing: Vec<String> = Vec::new();
    for (key, val) in wants {
        let already = existing.lines().any(|l| {
            let t = l.trim();
            t.starts_with(key) && t.contains('=')
        });
        if !already {
            missing.push(format!("  {key} = {val}\n"));
        }
    }
    if missing.is_empty() {
        return existing.to_owned();
    }

    match tui_header {
        Some(idx) => {
            let mut out: Vec<String> = lines.iter().map(|s| (*s).to_owned()).collect();
            let insert_at = idx + 1;
            for (offset, line) in missing.into_iter().enumerate() {
                out.insert(insert_at + offset, line);
            }
            out.concat()
        }
        None => {
            let mut out = existing.to_owned();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(
                "\n# Zap sidebar: emit OSC 9 even when the terminal is focused (Stop fallback).\n[tui]\n",
            );
            for line in missing {
                out.push_str(line.trim_start());
            }
            out
        }
    }
}

#[async_trait]
impl CliAgentPluginManager for CodexPluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        PLUGIN_VERSION
    }

    fn can_auto_install(&self) -> bool {
        true
    }

    fn is_installed(&self) -> bool {
        scripts_dir().join("on-stop.sh").is_file()
            && fs::read_to_string(hooks_json_path())
                .map(|s| s.contains(ZAP_MARKER))
                .unwrap_or(false)
    }

    fn needs_update(&self) -> bool {
        !self.is_installed()
    }

    fn supports_update(&self) -> bool {
        true
    }

    async fn install(&self) -> Result<(), PluginInstallError> {
        install_codex_hooks()
    }

    async fn update(&self) -> Result<(), PluginInstallError> {
        install_codex_hooks()
    }

    fn install_success_message(&self) -> &'static str {
        "Codex status hooks installed. Restart the Codex session (or /hooks) so PromptSubmit/Stop reach Zap."
    }

    fn update_success_message(&self) -> &'static str {
        "Codex status hooks updated. Restart the Codex session (or /hooks) to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }
}

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: "Install Codex status hooks",
    subtitle: "Zap writes hooks under ~/.codex so Codex emits sidebar Running/Done events.",
    steps: vec![PluginInstructionStep {
        description: "Hooks install automatically on Codex detect; restart Codex if status stays Idle.",
        command: "",
        executable: false,
        link: None,
    }],
    post_install_notes: vec![
        "After install, restart Codex so UserPromptSubmit/Stop OSC 777 events reach Zap.",
    ],
});

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
