use std::path::PathBuf;

use ai::skills::{home_skills_path, SkillProvider, SkillScope};

use super::{
    build_create_skill_user_query, can_advance, create_skill_symlinks, decide_create_skill_launch,
    display_skill_md_path, draft_skill_dir, draft_skill_md_path, find_name_collision,
    is_symlink_disabled, is_valid_skill_slug, planned_symlink_targets, skill_md_path,
    slugify_skill_name, symlink_candidate_providers, CreateSkillLaunchMode, SkillGenerateDraft,
    SkillGenerateIntentKind, SkillGenerateLocation, SkillGenerateStep,
};
use crate::ai::skills::{SkillInventoryDuplicate, SkillInventoryItem};

fn duplicate(name: &str, provider: SkillProvider, path: &str) -> SkillInventoryDuplicate {
    SkillInventoryDuplicate {
        path: PathBuf::from(path),
        name: name.into(),
        description: "desc".into(),
        content: "body".into(),
        provider,
        scope: SkillScope::Project,
    }
}

fn item(name: &str, duplicates: Vec<SkillInventoryDuplicate>) -> SkillInventoryItem {
    let default_skill = duplicates[0].clone();
    SkillInventoryItem {
        name: name.into(),
        default_skill,
        duplicates,
    }
}

#[test]
fn slugify_lowercases_and_hyphenates() {
    assert_eq!(slugify_skill_name("Summarize PR"), "summarize-pr");
    assert_eq!(slugify_skill_name("  Foo_Bar!! "), "foo-bar");
    assert_eq!(slugify_skill_name(""), "my-skill");
    assert_eq!(slugify_skill_name("---"), "my-skill");
}

#[test]
fn slug_validation_rejects_bad_names() {
    assert!(is_valid_skill_slug("summarize-pr"));
    assert!(is_valid_skill_slug("a"));
    assert!(is_valid_skill_slug("a1-b2"));
    assert!(!is_valid_skill_slug(""));
    assert!(!is_valid_skill_slug("-abc"));
    assert!(!is_valid_skill_slug("abc-"));
    assert!(!is_valid_skill_slug("Foo"));
    assert!(!is_valid_skill_slug("has space"));
    assert!(!is_valid_skill_slug("under_score"));
}

#[test]
fn default_location_is_home_agents() {
    assert_eq!(
        SkillGenerateDraft::default().location,
        SkillGenerateLocation::HomeAgents
    );
}

#[test]
fn home_agents_path_uses_agents_home() {
    let Some(home) = home_skills_path(SkillProvider::Agents) else {
        // CI without HOME is rare; skip soft.
        return;
    };
    let path = skill_md_path(SkillGenerateLocation::HomeAgents, "demo", None, "").unwrap();
    assert_eq!(path, home.join("demo").join("SKILL.md"));
}

#[test]
fn repo_agents_path_requires_root() {
    assert!(skill_md_path(SkillGenerateLocation::RepoAgents, "demo", None, "").is_none());
    let path = skill_md_path(
        SkillGenerateLocation::RepoAgents,
        "demo",
        Some(PathBuf::from("/repo").as_path()),
        "",
    )
    .unwrap();
    assert_eq!(
        path,
        PathBuf::from("/repo/.agents/skills/demo/SKILL.md")
    );
}

#[test]
fn custom_path_parent_and_skill_dir_forms() {
    let as_parent = skill_md_path(
        SkillGenerateLocation::Custom,
        "demo",
        None,
        "/tmp/my-skills",
    )
    .unwrap();
    assert_eq!(as_parent, PathBuf::from("/tmp/my-skills/demo/SKILL.md"));

    let as_skill_dir = skill_md_path(
        SkillGenerateLocation::Custom,
        "demo",
        None,
        "/tmp/my-skills/demo",
    )
    .unwrap();
    assert_eq!(as_skill_dir, PathBuf::from("/tmp/my-skills/demo/SKILL.md"));

    assert!(skill_md_path(SkillGenerateLocation::Custom, "demo", None, "").is_none());
    assert!(skill_md_path(SkillGenerateLocation::Custom, "demo", None, "   ").is_none());
}

#[test]
fn display_path_falls_back_when_no_repo() {
    let shown = display_skill_md_path(SkillGenerateLocation::RepoAgents, "x", None, "");
    assert!(shown.contains(".agents/skills/x/SKILL.md"));
}

#[test]
fn collision_finds_inventory_name() {
    let inventory = vec![item(
        "create-pr",
        vec![
            duplicate(
                "create-pr",
                SkillProvider::Agents,
                "/repo/.agents/skills/create-pr/SKILL.md",
            ),
            duplicate(
                "create-pr",
                SkillProvider::Claude,
                "/repo/.claude/skills/create-pr/SKILL.md",
            ),
        ],
    )];
    let hit = find_name_collision(&inventory, "create-pr").unwrap();
    assert_eq!(hit.name, "create-pr");
    assert_eq!(hit.effective.provider, SkillProvider::Agents);
    assert_eq!(hit.duplicates.len(), 2);
    assert!(find_name_collision(&inventory, "missing").is_none());
}

#[test]
fn create_skill_query_includes_path_overwrite_and_symlinks() {
    let draft = SkillGenerateDraft {
        goal: "总结 PR".into(),
        trigger_description: "提到 PR 总结时".into(),
        intent_kind: SkillGenerateIntentKind::Workflow,
        location: SkillGenerateLocation::HomeAgents,
        custom_path: String::new(),
        symlink_targets: vec![SkillProvider::Claude, SkillProvider::Codex],
        name: "summarize-pr".into(),
        allow_overwrite: true,
    };
    let path = PathBuf::from("/home/u/.agents/skills/summarize-pr/SKILL.md");
    let query = build_create_skill_user_query(&draft, &path);
    assert!(query.contains("create-skill"));
    assert!(query.contains("summarize-pr"));
    assert!(query.contains(path.to_str().unwrap()));
    assert!(query.contains("允许覆盖：是"));
    assert!(query.contains("先不跑完整 eval"));
    assert!(query.contains("写后软链"));
    assert!(query.contains("claude") || query.contains("Claude") || query.contains(".claude"));
}

#[test]
fn can_advance_gates_each_step() {
    let mut draft = SkillGenerateDraft::default();
    assert!(!can_advance(SkillGenerateStep::Intent, &draft, None));
    draft.goal = "目标".into();
    assert!(can_advance(SkillGenerateStep::Intent, &draft, None));

    assert!(!can_advance(SkillGenerateStep::Trigger, &draft, None));
    draft.trigger_description = "何时用".into();
    assert!(can_advance(SkillGenerateStep::Trigger, &draft, None));

    // 默认 HomeAgents：有 HOME 即可前进。
    if home_skills_path(SkillProvider::Agents).is_some() {
        assert!(can_advance(SkillGenerateStep::Location, &draft, None));
    }

    draft.location = SkillGenerateLocation::RepoAgents;
    assert!(!can_advance(SkillGenerateStep::Location, &draft, None));
    assert!(can_advance(
        SkillGenerateStep::Location,
        &draft,
        Some(PathBuf::from("/repo").as_path())
    ));

    draft.location = SkillGenerateLocation::Custom;
    assert!(!can_advance(SkillGenerateStep::Location, &draft, None));
    draft.custom_path = "~/projects/skills".into();
    assert!(can_advance(SkillGenerateStep::Location, &draft, None));

    draft.location = SkillGenerateLocation::RepoAgents;
    assert!(!can_advance(
        SkillGenerateStep::Name,
        &draft,
        Some(PathBuf::from("/repo").as_path())
    ));
    draft.name = "ok-skill".into();
    assert!(can_advance(
        SkillGenerateStep::Name,
        &draft,
        Some(PathBuf::from("/repo").as_path())
    ));
    assert!(can_advance(
        SkillGenerateStep::Launch,
        &draft,
        Some(PathBuf::from("/repo").as_path())
    ));
}

#[test]
fn personal_zap_primary_disables_zap_symlink() {
    let mut draft = SkillGenerateDraft {
        location: SkillGenerateLocation::PersonalZap,
        name: "demo".into(),
        symlink_targets: vec![SkillProvider::Zap, SkillProvider::Claude],
        ..SkillGenerateDraft::default()
    };
    assert!(is_symlink_disabled(
        SkillProvider::Zap,
        &draft,
        None
    ));
    assert!(!is_symlink_disabled(
        SkillProvider::Claude,
        &draft,
        None
    ));
    let planned = planned_symlink_targets(&draft, None);
    assert!(planned.iter().all(|p| !p.to_string_lossy().contains(".warp/skills/demo")
        || home_skills_path(SkillProvider::Zap).map(|h| !p.starts_with(h.join("demo"))).unwrap_or(true)));
    // Zap 被过滤掉
    if let Some(zap_home) = home_skills_path(SkillProvider::Zap) {
        assert!(!planned.contains(&zap_home.join("demo")));
    }
    draft.location = SkillGenerateLocation::HomeAgents;
    assert!(!is_symlink_disabled(SkillProvider::Zap, &draft, None));
}

#[test]
fn symlink_candidates_exclude_agents() {
    let candidates = symlink_candidate_providers();
    assert!(!candidates.contains(&SkillProvider::Agents));
    assert!(candidates.contains(&SkillProvider::Claude));
    assert!(candidates.contains(&SkillProvider::Codex));
    assert!(candidates.contains(&SkillProvider::Cursor));
    assert!(candidates.contains(&SkillProvider::Zap));
}

#[test]
fn create_skill_symlinks_links_and_skips_correct_existing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let primary = tmp.path().join("primary").join("my-skill");
    std::fs::create_dir_all(&primary).unwrap();
    std::fs::write(primary.join("SKILL.md"), "# hi").unwrap();

    let mirror_root = tmp.path().join("claude").join("skills");
    let target = mirror_root.join("my-skill");
    create_skill_symlinks(&primary, &[target.clone()]).unwrap();
    assert!(target.is_symlink());
    assert_eq!(
        std::fs::canonicalize(&target).unwrap(),
        std::fs::canonicalize(&primary).unwrap()
    );

    // 再次调用：已正确软链则跳过
    create_skill_symlinks(&primary, &[target.clone()]).unwrap();

    // 非匹配占用 → 错误
    let conflict = tmp.path().join("cursor").join("skills").join("my-skill");
    std::fs::create_dir_all(conflict.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&conflict).unwrap();
    let err = create_skill_symlinks(&primary, &[conflict]).unwrap_err();
    assert!(err.contains("已存在"));
}

#[test]
fn draft_helpers_resolve_home_agents() {
    let draft = SkillGenerateDraft {
        location: SkillGenerateLocation::HomeAgents,
        name: "x".into(),
        ..SkillGenerateDraft::default()
    };
    if home_skills_path(SkillProvider::Agents).is_some() {
        let md = draft_skill_md_path(&draft, None).unwrap();
        assert!(md.ends_with("x/SKILL.md"));
        let dir = draft_skill_dir(&draft, None).unwrap();
        assert!(dir.ends_with("x"));
    }
}

#[test]
fn step_navigation() {
    assert_eq!(SkillGenerateStep::Intent.next(), Some(SkillGenerateStep::Trigger));
    assert_eq!(SkillGenerateStep::Launch.next(), None);
    assert_eq!(SkillGenerateStep::Intent.prev(), None);
    assert_eq!(
        SkillGenerateStep::Launch.prev(),
        Some(SkillGenerateStep::Name)
    );
}

#[test]
fn decide_create_skill_launch_chooses_cli_when_byop_missing() {
    assert_eq!(
        decide_create_skill_launch(true),
        CreateSkillLaunchMode::ZapAgent
    );
    assert_eq!(
        decide_create_skill_launch(false),
        CreateSkillLaunchMode::CliFallback
    );
}
