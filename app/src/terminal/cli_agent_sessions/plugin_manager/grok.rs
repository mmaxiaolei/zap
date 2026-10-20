//! Grok Build TUI notification hooks for Zap sidebar status.
//!
//! Grok does not ship a Warp marketplace plugin. Instead we install hook JSON +
//! scripts under `~/.grok/hooks/` that emit OSC 777 `warp://cli-agent` events
//! (Stop / UserPromptSubmit / idle_prompt / …), matching Claude's protocol.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use async_trait::async_trait;

use super::{CliAgentPluginManager, PluginInstallError, PluginInstructionStep, PluginInstructions};

const PLUGIN_VERSION: &str = "1.0.0";

const HOOKS_JSON: &str = include_str!("../../../../assets/bundled/cli_agent_hooks/grok/zap-cli-agent.json");
const SCRIPT_BUILD_PAYLOAD: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/build-payload.sh");
const SCRIPT_ZAP_NOTIFY: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/zap-notify.sh");
const SCRIPT_ON_STOP: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/on-stop.sh");
const SCRIPT_ON_PROMPT: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/on-prompt-submit.sh");
const SCRIPT_ON_NOTIF: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/on-notification.sh");
const SCRIPT_ON_SESSION: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/on-session-start.sh");
const SCRIPT_ON_FAIL: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/on-stop-failure.sh");
const SCRIPT_ON_TURN_ENV: &str =
    include_str!("../../../../assets/bundled/cli_agent_hooks/grok/scripts/on-turn-complete-env.sh");

pub(super) struct GrokPluginManager;

fn grok_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".grok")
}

fn hooks_json_path() -> PathBuf {
    grok_home().join("hooks").join("zap-cli-agent.json")
}

fn scripts_dir() -> PathBuf {
    grok_home().join("hooks").join("zap-cli-agent").join("scripts")
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

/// Expand `$HOME` in the bundled hooks JSON to an absolute path for this user.
fn materialize_hooks_json() -> String {
    let home = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .into_owned();
    HOOKS_JSON.replace("$HOME", &home)
}

pub(crate) fn install_grok_hooks() -> Result<(), PluginInstallError> {
    let mut log = String::new();
    let scripts = scripts_dir();
    let files: &[(&str, &str)] = &[
        ("build-payload.sh", SCRIPT_BUILD_PAYLOAD),
        ("zap-notify.sh", SCRIPT_ZAP_NOTIFY),
        ("on-stop.sh", SCRIPT_ON_STOP),
        ("on-prompt-submit.sh", SCRIPT_ON_PROMPT),
        ("on-notification.sh", SCRIPT_ON_NOTIF),
        ("on-session-start.sh", SCRIPT_ON_SESSION),
        ("on-stop-failure.sh", SCRIPT_ON_FAIL),
        ("on-turn-complete-env.sh", SCRIPT_ON_TURN_ENV),
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
    let hooks_path = hooks_json_path();
    if let Some(parent) = hooks_path.parent() {
        fs::create_dir_all(parent).map_err(|err| PluginInstallError {
            message: format!("failed to create {}", parent.display()),
            log: format!("{log}error: {err}\n"),
        })?;
    }
    fs::write(&hooks_path, materialize_hooks_json()).map_err(|err| PluginInstallError {
        message: format!("failed to write {}", hooks_path.display()),
        log: format!("{log}error: {err}\n"),
    })?;
    log.push_str(&format!("wrote {}\n", hooks_path.display()));
    ensure_notification_hook_in_config(&mut log)?;
    let _ = log;
    Ok(())
}

fn ensure_notification_hook_in_config(log: &mut String) -> Result<(), PluginInstallError> {
    let config_path = grok_home().join("config.toml");
    let marker = "zap-cli-agent/scripts/on-turn-complete-env.sh";
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    if existing.contains(marker) {
        log.push_str("config.toml already has zap notification hook\n");
        return Ok(());
    }
    let script = scripts_dir().join("on-turn-complete-env.sh");
    let block = format!(
        "\n\n# Zap sidebar status: always emit OSC 777 on turn end (even when focused).\n[[ui.notifications.hooks]]\ncommand = \"{}\"\nevents = [\"turn_complete\", \"approval_required\", \"agent_error\", \"task_complete\", \"session_ready\"]\nonly_unfocused = false\ntimeout_secs = 5\n",
        script.display()
    );
    // Append; create file if missing.
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| PluginInstallError {
            message: format!("failed to create {}", parent.display()),
            log: format!("{log}error: {err}\n"),
        })?;
    }
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config_path)
        .map_err(|err| PluginInstallError {
            message: format!("failed to open {}", config_path.display()),
            log: format!("{log}error: {err}\n"),
        })?;
    file.write_all(block.as_bytes()).map_err(|err| PluginInstallError {
        message: format!("failed to update {}", config_path.display()),
        log: format!("{log}error: {err}\n"),
    })?;
    log.push_str(&format!("appended notification hook to {}\n", config_path.display()));
    Ok(())
}

#[async_trait]
impl CliAgentPluginManager for GrokPluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        PLUGIN_VERSION
    }

    fn can_auto_install(&self) -> bool {
        true
    }

    fn is_installed(&self) -> bool {
        hooks_json_path().is_file() && scripts_dir().join("on-stop.sh").is_file()
    }

    fn needs_update(&self) -> bool {
        // Marker file version: re-install when scripts dir is incomplete.
        !self.is_installed()
    }

    async fn install(&self) -> Result<(), PluginInstallError> {
        install_grok_hooks()
    }

    async fn update(&self) -> Result<(), PluginInstallError> {
        install_grok_hooks()
    }

    fn install_success_message(&self) -> &'static str {
        "Grok status hooks installed. Restart the Grok session (or /hooks reload) to activate."
    }

    fn update_success_message(&self) -> &'static str {
        "Grok status hooks updated. Restart the Grok session (or /hooks reload) to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }
}

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: "Install Grok status hooks",
    subtitle: "Zap writes hooks under ~/.grok/hooks so Grok emits sidebar status events.",
    steps: vec![PluginInstructionStep {
        description: "Hooks install automatically; restart Grok if the spinner stays stuck.",
        command: "",
        executable: false,
        link: None,
    }],
    post_install_notes: vec![
        "After install, restart the Grok session or run /hooks so Stop/idle_prompt reach Zap.",
    ],
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_hooks_json_expands_home() {
        let json = materialize_hooks_json();
        assert!(!json.contains("$HOME"));
        assert!(json.contains("zap-cli-agent/scripts/on-stop.sh"));
        assert!(json.contains("\"Stop\""));
    }

    #[test]
    fn can_auto_install_is_true() {
        assert!(GrokPluginManager.can_auto_install());
    }
}
