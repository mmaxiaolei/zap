use std::path::{Path, PathBuf};

use super::git::*;

struct GitFixture {
    tempdir: tempfile::TempDir,
    root: PathBuf,
    remote: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().unwrap();
        let root = tempdir.path().join("repo with 'quote");
        std::fs::create_dir(&root).unwrap();
        run_git(&root, &["init", "-b", "main"]);
        std::fs::write(root.join("README.md"), "fixture").unwrap();
        run_git(&root, &["add", "README.md"]);
        run_git(
            &root,
            &[
                "-c",
                "user.name=Zap Tests",
                "-c",
                "user.email=zap@example.com",
                "commit",
                "-m",
                "init",
            ],
        );

        let remote = tempdir.path().join("remote repository.git");
        let remote_str = remote.to_str().unwrap();
        run_git(
            tempdir.path(),
            &["init", "--bare", "-b", "main", remote_str],
        );
        run_git(&root, &["remote", "add", "origin", remote_str]);
        run_git(&root, &["push", "-u", "origin", "main"]);
        run_git(&root, &["remote", "set-head", "origin", "-a"]);

        Self {
            tempdir,
            root,
            remote,
        }
    }

    fn add_linked_worktree(&self, branch: &str) -> PathBuf {
        let path = self
            .tempdir
            .path()
            .join(format!("worktree {} 'quoted'", branch.replace('/', "-")));
        self.add_linked_worktree_at(branch, &path);
        path
    }

    fn add_linked_worktree_at(&self, branch: &str, path: &Path) {
        let output = command::blocking::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["worktree", "add", "-b", branch])
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "failed to add worktree for {branch}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = command::blocking::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn rejects_linked_worktree_as_repository() {
    let fixture = GitFixture::new();
    let worktree_path = fixture.add_linked_worktree("feature/a");

    let error = validate_repository(&worktree_path).unwrap_err();

    assert!(matches!(error, GitWorkspaceError::LinkedWorktree { .. }));
}

#[test]
fn rejects_directory_below_repository_root() {
    let fixture = GitFixture::new();
    let nested = fixture.root.join("nested");
    std::fs::create_dir(&nested).unwrap();

    let error = validate_repository(&nested).unwrap_err();

    assert!(matches!(error, GitWorkspaceError::NotRepositoryRoot { .. }));
}

#[test]
fn validates_repository_and_reads_remote_metadata() {
    let fixture = GitFixture::new();

    let repository = validate_repository(&fixture.root).unwrap();

    assert_eq!(repository.root, fixture.root.canonicalize().unwrap());
    assert_eq!(repository.remote, "origin");
    assert_eq!(repository.remote_url, fixture.remote.to_str().unwrap());
    assert!(matches!(
        repository.default_branch,
        BranchRef::Remote {
            remote,
            name,
            full_ref
        } if remote == "origin" && name == "main" && full_ref == "refs/remotes/origin/main"
    ));
}

#[cfg(unix)]
#[test]
fn decodes_non_utf8_git_path_output_without_loss() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let expected = PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/repository-\xff ".to_vec(),
    ));
    let mut output = expected.as_os_str().as_bytes().to_vec();
    output.push(b'\n');

    let decoded = decode_git_path_output(&output, "decode test path").unwrap();

    assert_eq!(
        decoded.as_os_str().as_bytes(),
        expected.as_os_str().as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn preserves_trailing_carriage_return_in_git_path_output() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let expected = PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/repository-with-trailing-cr\r".to_vec(),
    ));
    let mut output = expected.as_os_str().as_bytes().to_vec();
    output.push(b'\n');

    let decoded = decode_git_path_output(&output, "decode test path").unwrap();

    assert_eq!(
        decoded.as_os_str().as_bytes(),
        expected.as_os_str().as_bytes()
    );
}

#[test]
fn rejects_repository_without_remote() {
    let fixture = GitFixture::new();
    run_git(&fixture.root, &["remote", "remove", "origin"]);

    let error = validate_repository(&fixture.root).unwrap_err();

    assert!(matches!(error, GitWorkspaceError::RemoteNotFound { .. }));
}

#[test]
fn rejects_repository_without_remote_default_branch() {
    let fixture = GitFixture::new();
    run_git(
        &fixture.root,
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    );

    let error = validate_repository(&fixture.root).unwrap_err();

    assert!(matches!(
        error,
        GitWorkspaceError::DefaultBranchNotFound { remote, .. } if remote == "origin"
    ));
}

#[test]
fn selects_first_remote_when_origin_is_absent() {
    let fixture = GitFixture::new();
    let remote_url = fixture.remote.to_str().unwrap();
    run_git(&fixture.root, &["remote", "remove", "origin"]);
    run_git(&fixture.root, &["remote", "add", "a", remote_url]);
    run_git(
        &fixture.root,
        &["remote", "add", "zzzz-longer-remote", remote_url],
    );
    run_git(&fixture.root, &["fetch", "a"]);
    run_git(&fixture.root, &["remote", "set-head", "a", "-a"]);

    let repository = validate_repository(&fixture.root).unwrap();

    assert_eq!(repository.remote, "a");
    assert!(matches!(
        repository.default_branch,
        BranchRef::Remote { remote, name, .. } if remote == "a" && name == "main"
    ));
}

#[test]
fn classifies_local_and_remote_refs_without_prefix_guessing() {
    let fixture = GitFixture::new();
    run_git(&fixture.root, &["branch", "origin/foo"]);
    run_git(
        &fixture.root,
        &["push", "origin", "main:refs/heads/team/remote-branch"],
    );
    run_git(&fixture.root, &["fetch", "origin"]);

    let refs = list_branch_refs(&fixture.root).unwrap();

    assert!(refs.iter().any(|branch_ref| matches!(
        branch_ref,
        BranchRef::Local { name, full_ref }
            if name == "origin/foo" && full_ref == "refs/heads/origin/foo"
    )));
    assert!(refs.iter().any(|branch_ref| matches!(
        branch_ref,
        BranchRef::Remote {
            remote,
            name,
            full_ref
        } if remote == "origin"
            && name == "team/remote-branch"
            && full_ref == "refs/remotes/origin/team/remote-branch"
    )));
}

#[test]
fn rejects_ambiguous_overlapping_remote_ref() {
    let remotes = vec!["foo".to_string(), "foo/bar".to_string()];

    let error = parse_branch_ref("refs/remotes/foo/bar/main", &remotes).unwrap_err();

    assert!(matches!(
        error,
        GitWorkspaceError::AmbiguousRemoteRef {
            full_ref,
            remotes: candidates,
        } if full_ref == "refs/remotes/foo/bar/main" && candidates == remotes
    ));
}

#[test]
fn rejects_direct_head_ref_ambiguous_between_overlapping_remotes() {
    let fixture = GitFixture::new();
    let remote_url = fixture.remote.to_str().unwrap();
    run_git(&fixture.root, &["remote", "add", "foo", remote_url]);
    run_git(&fixture.root, &["remote", "add", "foo/bar", remote_url]);
    run_git(
        &fixture.root,
        &["update-ref", "refs/remotes/foo/bar/HEAD", "HEAD"],
    );

    let error = list_branch_refs(&fixture.root).unwrap_err();

    assert!(matches!(
        error,
        GitWorkspaceError::AmbiguousRemoteRef {
            full_ref,
            remotes,
        } if full_ref == "refs/remotes/foo/bar/HEAD"
            && remotes == ["foo".to_string(), "foo/bar".to_string()]
    ));
}

#[test]
fn rejects_malformed_branch_ref_record() {
    let error = parse_branch_ref_records("refs/heads/main\n", &[]).unwrap_err();

    assert!(matches!(
        error,
        GitWorkspaceError::InvalidBranchRefRecord { record }
            if record == "refs/heads/main"
    ));
}

#[test]
fn fetches_remote_refs_before_listing_them() {
    let fixture = GitFixture::new();
    run_git(
        &fixture.root,
        &["push", "origin", "main:refs/heads/created-after-clone"],
    );
    run_git(
        &fixture.root,
        &[
            "update-ref",
            "-d",
            "refs/remotes/origin/created-after-clone",
        ],
    );

    let refs = fetch_and_list_refs(&fixture.root).unwrap();

    assert!(refs.iter().any(|branch_ref| matches!(
        branch_ref,
        BranchRef::Remote { remote, name, .. }
            if remote == "origin" && name == "created-after-clone"
    )));
}

#[test]
fn fetches_primary_remote_when_branch_has_no_upstream() {
    let fixture = GitFixture::new();
    run_git(
        &fixture.root,
        &["push", "origin", "main:refs/heads/primary-only"],
    );

    let secondary = fixture.tempdir.path().join("secondary.git");
    run_git(
        fixture.tempdir.path(),
        &["init", "--bare", "-b", "main", secondary.to_str().unwrap()],
    );
    run_git(
        &fixture.root,
        &["remote", "add", "zz-secondary", secondary.to_str().unwrap()],
    );
    run_git(&fixture.root, &["push", "zz-secondary", "main"]);
    run_git(&fixture.root, &["remote", "remove", "origin"]);
    run_git(
        &fixture.root,
        &[
            "remote",
            "add",
            "a-primary",
            fixture.remote.to_str().unwrap(),
        ],
    );
    run_git(
        &fixture.root,
        &["config", "branch.main.remote", "zz-secondary"],
    );
    let _ = command::blocking::Command::new("git")
        .arg("-C")
        .arg(&fixture.root)
        .args(["config", "--unset-all", "branch.main.merge"])
        .status()
        .unwrap();

    let refs = fetch_and_list_refs(&fixture.root).unwrap();

    assert!(refs.iter().any(|branch_ref| matches!(
        branch_ref,
        BranchRef::Remote { remote, name, .. }
            if remote == "a-primary" && name == "primary-only"
    )));
}

#[test]
fn omits_symbolic_remote_head_from_ref_lists() {
    let fixture = GitFixture::new();

    let listed_refs = list_branch_refs(&fixture.root).unwrap();
    let fetched_refs = fetch_and_list_refs(&fixture.root).unwrap();

    for refs in [listed_refs, fetched_refs] {
        assert!(!refs.iter().any(|branch_ref| matches!(
            branch_ref,
            BranchRef::Remote { name, full_ref, .. }
                if name == "HEAD" || full_ref == "refs/remotes/origin/HEAD"
        )));
    }
}

#[test]
fn parses_worktree_paths_and_full_branch_refs() {
    let fixture = GitFixture::new();
    let linked_path = fixture.add_linked_worktree("feature/worktree");

    let worktrees = list_worktrees(&fixture.root).unwrap();

    assert!(worktrees.iter().any(|worktree| {
        worktree.path == fixture.root.canonicalize().unwrap()
            && worktree.branch.as_deref() == Some("refs/heads/main")
    }));
    assert!(worktrees.iter().any(|worktree| {
        worktree.path == linked_path.canonicalize().unwrap()
            && worktree.branch.as_deref() == Some("refs/heads/feature/worktree")
    }));
}

#[test]
fn preserves_prunable_worktree_when_its_path_no_longer_exists() {
    let fixture = GitFixture::new();
    let linked_path = fixture.add_linked_worktree("feature/prunable");
    std::fs::remove_dir_all(&linked_path).unwrap();

    let worktrees = list_worktrees(&fixture.root).unwrap();
    let expected_path = linked_path
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join(linked_path.file_name().unwrap());

    assert!(worktrees.iter().any(|worktree| {
        worktree.path == expected_path
            && worktree.branch.as_deref() == Some("refs/heads/feature/prunable")
            && worktree.is_prunable
    }));
}

#[test]
fn preserves_newline_worktree_path() {
    let fixture = GitFixture::new();
    let linked_path = fixture.tempdir.path().join("worktree\nnewline");
    fixture.add_linked_worktree_at("feature/newline", &linked_path);

    let worktrees = list_worktrees(&fixture.root).unwrap();

    assert!(worktrees
        .iter()
        .any(|worktree| worktree.path == linked_path.canonicalize().unwrap()));
}

#[cfg(unix)]
#[test]
fn preserves_non_utf8_worktree_path() {
    use std::os::unix::ffi::OsStringExt;

    let tempdir = tempfile::tempdir().unwrap();
    let linked_path = tempdir
        .path()
        .join(std::ffi::OsString::from_vec(b"worktree-\xff".to_vec()));
    let mut output = b"worktree ".to_vec();
    output.extend(linked_path.as_os_str().as_encoded_bytes());
    output.extend_from_slice(
        b"\0HEAD 0123456789abcdef\0branch refs/heads/feature/non-utf8\0prunable missing\0\0",
    );

    let worktrees = parse_worktrees(&output).unwrap();
    let expected_path = tempdir
        .path()
        .canonicalize()
        .unwrap()
        .join(std::ffi::OsString::from_vec(b"worktree-\xff".to_vec()));

    assert_eq!(worktrees[0].path, expected_path);
}

#[test]
fn clones_repository_into_path_with_spaces_and_quotes() {
    let fixture = GitFixture::new();
    let target = fixture.tempdir.path().join("clone path with 'quote");

    let repository = clone_repository(fixture.remote.to_str().unwrap(), Some(&target)).unwrap();

    assert_eq!(repository.root, target.canonicalize().unwrap());
    assert_eq!(repository.remote_url, fixture.remote.to_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(target.join("README.md")).unwrap(),
        "fixture"
    );
}

#[test]
fn clone_uses_repository_name_when_target_is_not_provided() {
    let fixture = GitFixture::new();
    let parent = fixture.tempdir.path().join("clone parent");
    std::fs::create_dir(&parent).unwrap();

    let repository =
        clone_repository_into(fixture.remote.to_str().unwrap(), &parent, None).unwrap();

    assert_eq!(
        repository.root,
        parent.join("remote repository").canonicalize().unwrap()
    );
}

#[test]
fn clone_into_rejects_invalid_directory_names_without_escaping_parent() {
    let fixture = GitFixture::new();
    let parent = fixture.tempdir.path().join("clone parent");
    std::fs::create_dir(&parent).unwrap();
    let escaped = fixture.tempdir.path().join("escaped");
    let absolute = fixture.tempdir.path().join("absolute-target");
    let invalid_names = [
        "../escaped".to_string(),
        absolute.to_string_lossy().into_owned(),
        "nested/name".to_string(),
        ".".to_string(),
        "".to_string(),
    ];

    for directory_name in invalid_names {
        let error = clone_repository_into(
            fixture.remote.to_str().unwrap(),
            &parent,
            Some(&directory_name),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            GitWorkspaceError::InvalidCloneDirectoryName { name }
                if name == directory_name
        ));
    }

    assert!(!escaped.exists());
    assert!(!absolute.exists());
    assert!(!parent.join("nested").exists());
}

#[test]
fn clone_into_accepts_single_normal_directory_name() {
    let fixture = GitFixture::new();
    let parent = fixture.tempdir.path().join("custom clone parent");
    std::fs::create_dir(&parent).unwrap();

    let repository = clone_repository_into(
        fixture.remote.to_str().unwrap(),
        &parent,
        Some("custom clone"),
    )
    .unwrap();

    assert_eq!(
        repository.root,
        parent.join("custom clone").canonicalize().unwrap()
    );
}

#[test]
fn clone_failure_removes_target_created_by_the_operation() {
    let tempdir = tempfile::tempdir().unwrap();
    let missing_source = tempdir.path().join("missing repository.git");
    let target = tempdir.path().join("new target");

    let error = clone_repository(missing_source.to_str().unwrap(), Some(&target)).unwrap_err();

    assert!(matches!(
        error,
        GitWorkspaceError::CommandFailed {
            operation,
            ref args,
            ref stderr,
        } if operation == "clone repository"
            && args.first().is_some_and(|arg| arg == "clone")
            && !stderr.is_empty()
    ));
    assert!(!target.exists());
}

#[test]
fn clone_never_deletes_preexisting_target() {
    let fixture = GitFixture::new();
    let target = fixture.tempdir.path().join("existing target");
    std::fs::create_dir(&target).unwrap();
    let sentinel = target.join("keep.txt");
    std::fs::write(&sentinel, "keep").unwrap();

    let error = clone_repository(fixture.remote.to_str().unwrap(), Some(&target)).unwrap_err();

    assert!(matches!(error, GitWorkspaceError::TargetExists { .. }));
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "keep");
}

#[test]
fn cleanup_failure_displays_clone_and_cleanup_errors() {
    let error = GitWorkspaceError::CleanupFailed {
        path: PathBuf::from("clone-target"),
        cleanup_source: std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "cleanup permission denied",
        ),
        clone_error: Box::new(GitWorkspaceError::CommandFailed {
            operation: "clone repository",
            args: vec!["clone".to_string()],
            stderr: "fatal: source repository missing".to_string(),
        }),
    };

    let message = error.to_string();

    assert!(message.contains("fatal: source repository missing"));
    assert!(message.contains("cleanup permission denied"));
}

#[test]
fn parses_repository_names_from_supported_git_urls() {
    assert_eq!(
        repository_name_from_url("https://github.com/acme/widgets.git").unwrap(),
        "widgets"
    );
    assert_eq!(
        repository_name_from_url("git@github.com:acme/widgets.git").unwrap(),
        "widgets"
    );
    assert_eq!(
        repository_name_from_url("ssh://git@example.com/acme/widgets/").unwrap(),
        "widgets"
    );
    assert_eq!(
        repository_name_from_url("/tmp/repositories/local-widgets.git").unwrap(),
        "local-widgets"
    );
}

#[test]
fn rejects_git_url_without_repository_name() {
    let error = repository_name_from_url("ssh://git@example.com/").unwrap_err();

    assert!(matches!(
        error,
        GitWorkspaceError::RepositoryNameMissing { .. }
    ));
}

#[test]
fn parses_repository_name_from_windows_drive_path() {
    assert_eq!(
        repository_name_from_url(r"C:\repositories\windows-widgets.git").unwrap(),
        "windows-widgets"
    );
}

#[test]
fn creates_filesystem_safe_workspace_directory_names() {
    assert_eq!(
        workspace_dir_name("feature/a b", "12345678"),
        "feature-a-b-12345678"
    );
    assert_eq!(
        workspace_dir_name("  feature\\a::b///c  ", "abcdef12-extra"),
        "feature-a-b-c-abcdef12"
    );
    assert_eq!(
        workspace_dir_name("///:::   ", "fedcba98"),
        "workspace-fedcba98"
    );
}

#[tokio::test]
async fn async_wrappers_run_git_operations_off_the_calling_task() {
    let fixture = GitFixture::new();
    let target = fixture.tempdir.path().join("async clone");

    let validated = validate_repository_async(fixture.root.clone())
        .await
        .unwrap();
    let refs = fetch_and_list_refs_async(fixture.root.clone())
        .await
        .unwrap();
    let worktrees = list_worktrees_async(fixture.root.clone()).await.unwrap();
    let cloned = clone_repository_async(
        fixture.remote.to_str().unwrap().to_string(),
        Some(target.clone()),
    )
    .await
    .unwrap();

    assert_eq!(validated.root, fixture.root.canonicalize().unwrap());
    assert!(!refs.is_empty());
    assert_eq!(worktrees.len(), 1);
    assert_eq!(cloned.root, target.canonicalize().unwrap());
}
