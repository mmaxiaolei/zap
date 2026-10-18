//! Skill 生成向导：落点/命名/撞名与 create-skill 预填 query。

use std::path::{Path, PathBuf};

use ai::skills::{home_skills_path, SkillProvider};

use crate::ai::skills::{SkillInventoryDuplicate, SkillInventoryItem};

/// 向导选择的写入落点（主落点，真正写入 SKILL.md 的地方）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillGenerateLocation {
    /// 家目录 `~/.agents/skills/<name>/SKILL.md`（默认）
    HomeAgents,
    /// 当前仓库 `.agents/skills/<name>/SKILL.md`
    RepoAgents,
    /// 个人目录（渠道相关，通常 `~/.warp/skills` 或 `~/.zap/skills`）
    PersonalZap,
    /// 用户自定义 skills 父目录（或已以 skill 名结尾的 skill 目录）
    Custom,
}

impl SkillGenerateLocation {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::HomeAgents => "skill-generate-location-home-agents",
            Self::RepoAgents => "skill-generate-location-agents",
            Self::PersonalZap => "skill-generate-location-personal",
            Self::Custom => "skill-generate-location-custom",
        }
    }

    pub fn all() -> [Self; 4] {
        [
            Self::HomeAgents,
            Self::RepoAgents,
            Self::PersonalZap,
            Self::Custom,
        ]
    }
}

/// 意图类型提示（只进预填 query，不当最终正文）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SkillGenerateIntentKind {
    #[default]
    Workflow,
    Review,
    Docs,
    Other,
}

impl SkillGenerateIntentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workflow => "workflow",
            Self::Review => "review",
            Self::Docs => "docs",
            Self::Other => "other",
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::Workflow, Self::Review, Self::Docs, Self::Other]
    }
}

/// 向导收集步骤（启动 AI 前）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillGenerateStep {
    Intent = 0,
    Trigger = 1,
    Location = 2,
    Name = 3,
    Launch = 4,
}

impl SkillGenerateStep {
    pub const COUNT: usize = 5;

    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Intent),
            1 => Some(Self::Trigger),
            2 => Some(Self::Location),
            3 => Some(Self::Name),
            4 => Some(Self::Launch),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn next(self) -> Option<Self> {
        Self::from_index(self.index() + 1)
    }

    pub fn prev(self) -> Option<Self> {
        self.index().checked_sub(1).and_then(Self::from_index)
    }
}

/// 向导整体阶段。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillGeneratePhase {
    Closed,
    Collecting {
        step: SkillGenerateStep,
        /// 目标路径已存在时，启动前需硬确认覆盖。
        awaiting_overwrite_confirm: bool,
    },
    Drafting,
    /// 未配置 BYOP：展示可复制 Prompt，用户贴到自己的 agent CLI。
    CliFallback,
    Review {
        path: PathBuf,
        /// 打开已存在文件前再确认一次覆盖语义（agent 可能已写入）。
        path_exists: bool,
        awaiting_overwrite_confirm: bool,
    },
}

impl Default for SkillGeneratePhase {
    fn default() -> Self {
        Self::Closed
    }
}

/// 向导草稿（启动 AI 所需字段）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillGenerateDraft {
    pub goal: String,
    pub trigger_description: String,
    pub intent_kind: SkillGenerateIntentKind,
    pub location: SkillGenerateLocation,
    /// Custom 落点：skills 父目录（或已以 skill 名结尾的目录）。空则不可前进。
    pub custom_path: String,
    /// 写盘成功后要软链到的 provider 家目录（不含 Agents；主落点已是某目录时该项禁用）。
    pub symlink_targets: Vec<SkillProvider>,
    pub name: String,
    pub allow_overwrite: bool,
}

impl Default for SkillGenerateDraft {
    fn default() -> Self {
        Self {
            goal: String::new(),
            trigger_description: String::new(),
            intent_kind: SkillGenerateIntentKind::Workflow,
            location: SkillGenerateLocation::HomeAgents,
            custom_path: String::new(),
            symlink_targets: Vec::new(),
            name: String::new(),
            allow_overwrite: false,
        }
    }
}

/// 同名库存命中信息（命名步软告警）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillNameCollision {
    pub name: String,
    pub effective: SkillInventoryDuplicate,
    pub duplicates: Vec<SkillInventoryDuplicate>,
}

/// 将用户输入规范为 skill slug（kebab-case）。
pub fn slugify_skill_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_hyphen = true;
    for ch in raw.trim().chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_hyphen = false;
        } else if !last_hyphen {
            out.push('-');
            last_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "my-skill".to_string()
    } else {
        out.chars().take(64).collect()
    }
}

/// slug 是否合法：`^[a-z0-9]+(-[a-z0-9]+)*$`，长度 1..=64。
pub fn is_valid_skill_slug(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            continue;
        }
        if ch == '-' {
            match chars.peek() {
                Some(next) if next.is_ascii_lowercase() || next.is_ascii_digit() => continue,
                _ => return false,
            }
        } else {
            return false;
        }
    }
    !name.starts_with('-') && !name.ends_with('-')
}

fn expand_user_path(raw: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(raw.trim()).into_owned())
}

/// 自定义路径 → skill 目录：若末段已是 skill 名则直接用；否则视为 skills 父目录。
fn custom_skill_dir(custom_path: &str, name: &str) -> Option<PathBuf> {
    let trimmed = custom_path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let expanded = expand_user_path(trimmed);
    let last = expanded
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if last == name {
        Some(expanded)
    } else {
        Some(expanded.join(name))
    }
}

/// 解析目标 `SKILL.md` 绝对路径。
///
/// - [`SkillGenerateLocation::HomeAgents`] 使用 `home_skills_path(Agents)`
/// - [`SkillGenerateLocation::RepoAgents`] 需要 `repo_root`
/// - [`SkillGenerateLocation::PersonalZap`] 使用 `home_skills_path(Zap)`
/// - [`SkillGenerateLocation::Custom`] 需要非空 `custom_path`
pub fn skill_md_path(
    location: SkillGenerateLocation,
    name: &str,
    repo_root: Option<&Path>,
    custom_path: &str,
) -> Option<PathBuf> {
    if !is_valid_skill_slug(name) {
        return None;
    }
    match location {
        SkillGenerateLocation::HomeAgents => {
            let home = home_skills_path(SkillProvider::Agents)?;
            Some(home.join(name).join("SKILL.md"))
        }
        SkillGenerateLocation::RepoAgents => {
            let root = repo_root?;
            Some(
                root.join(".agents")
                    .join("skills")
                    .join(name)
                    .join("SKILL.md"),
            )
        }
        SkillGenerateLocation::PersonalZap => {
            let home = home_skills_path(SkillProvider::Zap)?;
            Some(home.join(name).join("SKILL.md"))
        }
        SkillGenerateLocation::Custom => {
            let dir = custom_skill_dir(custom_path, name)?;
            Some(dir.join("SKILL.md"))
        }
    }
}

/// 从草稿解析目标 `SKILL.md`。
pub fn draft_skill_md_path(
    draft: &SkillGenerateDraft,
    repo_root: Option<&Path>,
) -> Option<PathBuf> {
    skill_md_path(
        draft.location,
        &draft.name,
        repo_root,
        &draft.custom_path,
    )
}

/// 主落点 skill 目录（`…/<name>/`，不含 SKILL.md）。
pub fn draft_skill_dir(draft: &SkillGenerateDraft, repo_root: Option<&Path>) -> Option<PathBuf> {
    draft_skill_md_path(draft, repo_root).and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

/// 路径展示用字符串：有绝对路径用绝对；缺上下文时用相对 / `~` 模式。
pub fn display_skill_md_path(
    location: SkillGenerateLocation,
    name: &str,
    repo_root: Option<&Path>,
    custom_path: &str,
) -> String {
    let slug = if is_valid_skill_slug(name) {
        name.to_string()
    } else if name.trim().is_empty() {
        "name".to_string()
    } else {
        slugify_skill_name(name)
    };
    if let Some(path) = skill_md_path(location, &slug, repo_root, custom_path) {
        return path.display().to_string();
    }
    match location {
        SkillGenerateLocation::HomeAgents => {
            format!("~/.agents/skills/{slug}/SKILL.md")
        }
        SkillGenerateLocation::RepoAgents => {
            format!("（当前仓库）/.agents/skills/{slug}/SKILL.md")
        }
        SkillGenerateLocation::PersonalZap => {
            format!("~/.warp/skills/{slug}/SKILL.md")
        }
        SkillGenerateLocation::Custom => {
            let raw = custom_path.trim();
            if raw.is_empty() {
                format!("（自定义）/{slug}/SKILL.md")
            } else {
                let dir = custom_skill_dir(raw, &slug)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| format!("{raw}/{slug}"));
                format!("{dir}/SKILL.md")
            }
        }
    }
}

/// 软链候选：Claude / Codex / Cursor / 个人 Zap。不含 Agents（主落点默认已是家目录 Agents）。
pub fn symlink_candidate_providers() -> [SkillProvider; 4] {
    [
        SkillProvider::Claude,
        SkillProvider::Codex,
        SkillProvider::Cursor,
        SkillProvider::Zap,
    ]
}

fn paths_same_dir(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// 某 provider 的软链勾选是否应禁用（主落点已是该目录，避免自链）。
pub fn is_symlink_disabled(
    provider: SkillProvider,
    draft: &SkillGenerateDraft,
    repo_root: Option<&Path>,
) -> bool {
    if draft.location == SkillGenerateLocation::PersonalZap && provider == SkillProvider::Zap {
        return true;
    }
    let Some(primary_dir) = draft_skill_dir(draft, repo_root) else {
        return false;
    };
    let Some(home) = home_skills_path(provider) else {
        return true;
    };
    let target = home.join(&draft.name);
    paths_same_dir(&primary_dir, &target)
        || primary_dir.starts_with(&home) && primary_dir == target
}

/// 解析已勾选且未禁用的软链目标路径（`<provider_home>/<name>`）。
pub fn planned_symlink_targets(
    draft: &SkillGenerateDraft,
    repo_root: Option<&Path>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for provider in &draft.symlink_targets {
        if is_symlink_disabled(*provider, draft, repo_root) {
            continue;
        }
        if let Some(home) = home_skills_path(*provider) {
            out.push(home.join(&draft.name));
        }
    }
    out
}

fn display_symlink_target(provider: SkillProvider, name: &str) -> String {
    let slug = if is_valid_skill_slug(name) {
        name.to_string()
    } else if name.trim().is_empty() {
        "<name>".to_string()
    } else {
        slugify_skill_name(name)
    };
    if let Some(home) = home_skills_path(provider) {
        return home.join(&slug).display().to_string();
    }
    let home_hint = match provider {
        SkillProvider::Claude => "~/.claude/skills",
        SkillProvider::Codex => "~/.codex/skills",
        SkillProvider::Cursor => "~/.cursor/skills",
        SkillProvider::Zap => "~/.warp/skills",
        SkillProvider::Agents => "~/.agents/skills",
        _ => "~/.?/skills",
    };
    format!("{home_hint}/{slug}")
}

/// 在主落点 skill 目录存在后，为其创建软链到各目标。
///
/// - 目标已是指向主落点的软链 → 跳过
/// - 目标已存在且不是匹配软链 → 返回错误（不静默覆盖）
/// - 否则 `symlink(primary → target)`
pub fn create_skill_symlinks(
    primary_skill_dir: &Path,
    targets: &[PathBuf],
) -> Result<(), String> {
    for target in targets {
        if target.as_path() == primary_skill_dir
            || paths_same_dir(target, primary_skill_dir)
        {
            continue;
        }
        match std::fs::symlink_metadata(target) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let link = std::fs::read_link(target).map_err(|e| {
                    format!("无法读取软链 {}: {e}", target.display())
                })?;
                let resolved = if link.is_absolute() {
                    link
                } else {
                    target
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(link)
                };
                if paths_same_dir(&resolved, primary_skill_dir) || resolved == primary_skill_dir {
                    continue;
                }
                return Err(format!(
                    "目标已存在软链但未指向主落点: {} → {}（期望 {}）",
                    target.display(),
                    resolved.display(),
                    primary_skill_dir.display()
                ));
            }
            Ok(_) => {
                return Err(format!(
                    "目标已存在且不是指向主落点的软链: {}",
                    target.display()
                ));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!("无法检查目标 {}: {err}", target.display()));
            }
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("无法创建软链父目录 {}: {e}", parent.display())
            })?;
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(primary_skill_dir, target).map_err(|e| {
                format!(
                    "创建软链失败 {} → {}: {e}",
                    target.display(),
                    primary_skill_dir.display()
                )
            })?;
        }
        #[cfg(not(unix))]
        {
            return Err(format!(
                "当前平台不支持软链: {} → {}",
                target.display(),
                primary_skill_dir.display()
            ));
        }
    }
    Ok(())
}

/// 库存同名检测（含任意 provider）。
pub fn find_name_collision<'a>(
    inventory: &'a [SkillInventoryItem],
    name: &str,
) -> Option<SkillNameCollision> {
    let item = inventory.iter().find(|item| item.name == name)?;
    Some(SkillNameCollision {
        name: item.name.clone(),
        effective: item.default_skill.clone(),
        duplicates: item.duplicates.clone(),
    })
}

/// 目标路径是否已有文件（写入前硬确认）。
pub fn target_path_exists(path: &Path) -> bool {
    path.is_file()
}

/// 构建发给 create-skill 的预填 `user_query`。
pub fn build_create_skill_user_query(draft: &SkillGenerateDraft, target_path: &Path) -> String {
    let overwrite = if draft.allow_overwrite {
        "允许覆盖：是（写入前仍请先向用户确认结构，再覆盖已有 SKILL.md）"
    } else {
        "允许覆盖：否（若路径已存在，请停止写入并询问用户）"
    };
    let symlink_lines: Vec<String> = draft
        .symlink_targets
        .iter()
        .filter(|p| {
            // 查询阶段可能尚无完整 path；仍列出用户勾选且非自链的项。
            !(draft.location == SkillGenerateLocation::PersonalZap && **p == SkillProvider::Zap)
        })
        .map(|p| format!("  - {}", display_symlink_target(*p, draft.name.trim())))
        .collect();
    let symlink_block = if symlink_lines.is_empty() {
        "· 写后软链：无（Zap 在确认打开时也不会创建额外软链）".to_string()
    } else {
        format!(
            "· 写后软链（由 Zap 在确认打开 / 写盘成功后创建，勿在草拟阶段提前 ln）：\n{}",
            symlink_lines.join("\n")
        )
    };
    format!(
        "请按 create-skill 为我起草一个新 skill。\n\
         · 目标：{goal}\n\
         · 类型提示：{intent}\n\
         · description 要点：{trigger}\n\
         · 建议 name：{name}\n\
         · 写入：{path}\n\
         · {overwrite}\n\
         {symlink_block}\n\
         · 语言：中文说明可写进 skill 正文与 description\n\
         · 先产出可运行的单文件 SKILL.md；除非我明确要求，否则先不跑完整 eval / benchmark",
        goal = draft.goal.trim(),
        intent = draft.intent_kind.as_str(),
        trigger = draft.trigger_description.trim(),
        name = draft.name.trim(),
        path = target_path.display(),
        overwrite = overwrite,
        symlink_block = symlink_block,
    )
}

/// 启动 create-skill 时：有可用 BYOP 走 Zap 内 Agent；否则走 CLI 复制兜底。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateSkillLaunchMode {
    ZapAgent,
    CliFallback,
}

pub fn decide_create_skill_launch(byop_configured: bool) -> CreateSkillLaunchMode {
    if byop_configured {
        CreateSkillLaunchMode::ZapAgent
    } else {
        CreateSkillLaunchMode::CliFallback
    }
}

/// 收集步是否可进入下一步。
pub fn can_advance(step: SkillGenerateStep, draft: &SkillGenerateDraft, repo_root: Option<&Path>) -> bool {
    match step {
        SkillGenerateStep::Intent => !draft.goal.trim().is_empty(),
        SkillGenerateStep::Trigger => !draft.trigger_description.trim().is_empty(),
        SkillGenerateStep::Location => match draft.location {
            SkillGenerateLocation::HomeAgents => {
                home_skills_path(SkillProvider::Agents).is_some()
            }
            SkillGenerateLocation::RepoAgents => repo_root.is_some(),
            SkillGenerateLocation::PersonalZap => {
                home_skills_path(SkillProvider::Zap).is_some()
            }
            SkillGenerateLocation::Custom => !draft.custom_path.trim().is_empty(),
        },
        SkillGenerateStep::Name => is_valid_skill_slug(&draft.name),
        SkillGenerateStep::Launch => {
            is_valid_skill_slug(&draft.name)
                && draft_skill_md_path(draft, repo_root).is_some()
        }
    }
}

#[cfg(test)]
#[path = "generate_tests.rs"]
mod tests;
