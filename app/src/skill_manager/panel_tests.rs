use std::path::PathBuf;

use ai::skills::{SkillProvider, SkillScope};

use super::{SkillInventoryDuplicate, SkillInventoryItem, SkillManagerPanel};

fn duplicate(
    provider: SkillProvider,
    path: &str,
    description: &str,
    content: &str,
) -> SkillInventoryDuplicate {
    SkillInventoryDuplicate {
        path: PathBuf::from(path),
        name: "drawio".into(),
        description: description.into(),
        content: content.into(),
        provider,
        scope: SkillScope::Project,
    }
}

fn item(duplicates: Vec<SkillInventoryDuplicate>) -> SkillInventoryItem {
    let default_skill = duplicates[0].clone();
    SkillInventoryItem {
        name: "drawio".into(),
        default_skill,
        duplicates,
    }
}

#[test]
fn filter_keeps_effective_source_when_it_still_matches() {
    let agents = duplicate(
        SkillProvider::Agents,
        "/agents/SKILL.md",
        "diagrams",
        "same",
    );
    let claude = duplicate(
        SkillProvider::Claude,
        "/claude/SKILL.md",
        "diagrams",
        "same",
    );
    let inventory = vec![item(vec![agents.clone(), claude])];

    let filtered = SkillManagerPanel::filter_inventory(&inventory, "diagram", None);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].duplicates.len(), 2);
    assert_eq!(filtered[0].default_skill.path, agents.path);
}

#[test]
fn filter_falls_back_when_effective_source_is_excluded() {
    let agents = duplicate(SkillProvider::Agents, "/agents/SKILL.md", "alpha", "alpha");
    let claude = duplicate(SkillProvider::Claude, "/claude/SKILL.md", "beta", "beta");
    let inventory = vec![item(vec![agents, claude.clone()])];

    let filtered = SkillManagerPanel::filter_inventory(&inventory, "beta", None);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].duplicates.len(), 1);
    assert_eq!(filtered[0].default_skill.path, claude.path);
    assert!(!filtered[0].has_duplicates());
}

#[test]
fn shown_duplicate_prefers_hover_then_pin_then_effective() {
    let agents = duplicate(SkillProvider::Agents, "/agents/SKILL.md", "base", "base");
    let claude = duplicate(SkillProvider::Claude, "/claude/SKILL.md", "other", "other");
    let codex = duplicate(SkillProvider::Codex, "/codex/SKILL.md", "base", "base");
    let inventory = item(vec![agents.clone(), claude.clone(), codex.clone()]);

    let pinned =
        SkillManagerPanel::resolve_shown_duplicate(&inventory, Some(claude.path.as_path()), None);
    assert_eq!(pinned.path, claude.path);

    let hovered = SkillManagerPanel::resolve_shown_duplicate(
        &inventory,
        Some(claude.path.as_path()),
        Some(codex.path.as_path()),
    );
    assert_eq!(hovered.path, codex.path);

    let missing = PathBuf::from("/missing");
    let also_missing = PathBuf::from("/also-missing");
    let fallback = SkillManagerPanel::resolve_shown_duplicate(
        &inventory,
        Some(missing.as_path()),
        Some(also_missing.as_path()),
    );
    assert_eq!(fallback.path, agents.path);
}

#[test]
fn content_diff_dot_ignores_identical_copies_and_the_effective_source() {
    let agents = duplicate(SkillProvider::Agents, "/agents/SKILL.md", "base", "base");
    let same = duplicate(SkillProvider::Claude, "/claude/SKILL.md", "base", "base");
    let changed = duplicate(SkillProvider::Codex, "/codex/SKILL.md", "base", "fork");

    assert!(!SkillManagerPanel::source_content_differs(&agents, &agents));
    assert!(!SkillManagerPanel::source_content_differs(&same, &agents));
    assert!(SkillManagerPanel::source_content_differs(&changed, &agents));
}
