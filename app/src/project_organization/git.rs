use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Component, Path, PathBuf},
    process::Output,
};

use thiserror::Error;

mod ref_transaction;

/// Git 分支引用，保留完整 refname 以避免通过名称前缀猜测类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchRef {
    Local {
        name: String,
        full_ref: String,
    },
    Remote {
        remote: String,
        name: String,
        full_ref: String,
    },
}

/// 已通过主工作目录、remote 和默认分支校验的 repository 元数据。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRepository {
    pub root: PathBuf,
    pub remote: String,
    pub remote_url: String,
    pub default_branch: BranchRef,
}

/// `git worktree list --porcelain` 返回的 worktree 信息。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    pub locked_reason: Option<String>,
    pub is_prunable: bool,
    pub prunable_reason: Option<String>,
}

/// 删除 linked worktree 前的只读校验结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeletionPreflight {
    /// `list_worktrees` 中已验证的 canonical registered path。
    pub worktree_path: PathBuf,
    /// Worktree 当前检出的本地分支短名称。
    pub branch: String,
    /// Preflight 时本地分支指向的精确 commit OID。
    pub branch_oid: String,
    /// 分支是否已合入 `merge_target`；未请求删分支时固定为 `true`。
    pub is_merged: bool,
    /// 用于 merge 判断的完整 ref；未请求删分支时为空字符串。
    pub merge_target: String,
}

/// Repository workspace Git 操作的结构化错误。
#[derive(Debug, Error)]
pub enum GitWorkspaceError {
    #[error("failed to canonicalize `{path}`: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("selected path `{selected}` is not repository root `{root}`")]
    NotRepositoryRoot { selected: PathBuf, root: PathBuf },
    #[error(
        "linked worktree cannot be registered as a repository: git dir `{git_dir}`, common dir `{common_dir}`"
    )]
    LinkedWorktree {
        git_dir: PathBuf,
        common_dir: PathBuf,
    },
    #[error("repository `{repo}` has no configured remote")]
    RemoteNotFound { repo: PathBuf },
    #[error("remote `{remote}` in repository `{repo}` has no default branch: {stderr}")]
    DefaultBranchNotFound {
        repo: PathBuf,
        remote: String,
        stderr: String,
    },
    #[error("failed to execute git for {operation} with arguments {args:?}: {source}")]
    CommandIo {
        operation: &'static str,
        args: Vec<String>,
        #[source]
        source: io::Error,
    },
    #[error("git failed to {operation} with arguments {args:?}: {stderr}")]
    CommandFailed {
        operation: &'static str,
        args: Vec<String>,
        stderr: String,
    },
    #[error("git returned invalid UTF-8 while attempting to {operation}")]
    InvalidUtf8 { operation: &'static str },
    #[error("git returned invalid branch ref `{full_ref}`")]
    InvalidBranchRef { full_ref: String },
    #[error("selected remote ref `{full_ref}` is not a direct ref of a configured remote")]
    InvalidRemoteRef { full_ref: String },
    #[error("branch ref `{full_ref}` does not exist")]
    BranchNotFound { full_ref: String },
    #[error("branch name `{branch}` is invalid")]
    InvalidBranchName { branch: String },
    #[error("local branch `{branch}` already exists")]
    BranchAlreadyExists { branch: String },
    #[error(
        "failed to atomically claim local branch `{branch}` at expected OID {expected_oid}: {claim_error}"
    )]
    BranchClaimFailed {
        branch: String,
        expected_oid: String,
        claim_error: Box<GitWorkspaceError>,
    },
    #[error("local branch `{branch}` is already checked out at `{path}`")]
    BranchAlreadyCheckedOut { branch: String, path: PathBuf },
    #[error("worktree `{path}` is not registered in the repository")]
    WorktreeNotFound { path: PathBuf },
    #[error("worktree `{path}` appears more than once in the repository")]
    AmbiguousWorktree { path: PathBuf },
    #[error("worktree `{path}` does not check out a local branch")]
    WorktreeHasNoLocalBranch { path: PathBuf },
    #[error("worktree `{path}` contains uncommitted changes")]
    DirtyWorktree { path: PathBuf },
    #[error("worktree branch mismatch: expected `{expected}`, found `{actual}`")]
    WorktreeBranchMismatch { expected: String, actual: String },
    #[error("branch `{branch}` is not merged into `{merge_target}`")]
    BranchNotMerged {
        branch: String,
        merge_target: String,
    },
    #[error(
        "branch `{branch}` changed after preflight: expected {expected_oid}, found {actual_oid:?}"
    )]
    BranchChanged {
        branch: String,
        expected_oid: String,
        actual_oid: Option<String>,
        actual_symbolic_target: Option<String>,
    },
    #[error(
        "git reported success deleting branch ref `{full_ref}` at expected OID {expected_oid}, but deletion could not be confirmed"
    )]
    BranchDeleteNotCompleted {
        full_ref: String,
        expected_oid: String,
    },
    #[error(
        "failed to clean up branch ref `{full_ref}` at expected OID {expected_oid}: {delete_error}; current direct OID {actual_oid:?}, symbolic target {actual_symbolic_target:?}, inspection error {inspection_error:?}"
    )]
    BranchCleanupFailed {
        full_ref: String,
        expected_oid: String,
        actual_oid: Option<String>,
        actual_symbolic_target: Option<String>,
        delete_error: Box<GitWorkspaceError>,
        inspection_error: Option<Box<GitWorkspaceError>>,
    },
    #[error(
        "worktree `{worktree_path}` was removed, but deleting branch `{branch}` at expected OID {expected_oid} failed: {delete_error}; current direct OID {actual_oid:?}, symbolic target {actual_symbolic_target:?}, inspection error {inspection_error:?}"
    )]
    BranchDeleteFailed {
        worktree_path: PathBuf,
        worktree_removed: bool,
        branch: String,
        expected_oid: String,
        actual_oid: Option<String>,
        actual_symbolic_target: Option<String>,
        delete_error: Box<GitWorkspaceError>,
        inspection_error: Option<Box<GitWorkspaceError>>,
    },
    #[error("git returned invalid branch ref record `{record}`")]
    InvalidBranchRefRecord { record: String },
    #[error("remote ref `{full_ref}` matches multiple remotes: {remotes:?}")]
    AmbiguousRemoteRef {
        full_ref: String,
        remotes: Vec<String>,
    },
    #[error("git returned invalid worktree record: {record}")]
    InvalidWorktreeRecord { record: String },
    #[error("target `{path}` already exists")]
    TargetExists { path: PathBuf },
    #[error("failed to inspect target `{path}`: {source}")]
    TargetInspection {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "worktree creation for branch `{branch}` failed and the branch changed from expected OID {expected_oid} to direct OID {actual_oid:?}, symbolic target {actual_symbolic_target:?}: {create_error}"
    )]
    WorktreeCreationBranchChanged {
        branch: String,
        expected_oid: String,
        actual_oid: Option<String>,
        actual_symbolic_target: Option<String>,
        create_error: Box<GitWorkspaceError>,
    },
    #[error(
        "worktree creation for branch `{branch}` failed: {create_error}; branch cleanup also failed: {cleanup_error}"
    )]
    WorktreeCreationCleanupFailed {
        branch: String,
        create_error: Box<GitWorkspaceError>,
        cleanup_error: Box<GitWorkspaceError>,
    },
    #[error(
        "worktree `{worktree_path}` for branch `{branch}` was created but could not be verified at expected OID {expected_oid}; the worktree and branch may remain: {verification_error}"
    )]
    WorktreeCreationVerificationFailed {
        worktree_path: PathBuf,
        branch: String,
        expected_oid: String,
        verification_error: Box<GitWorkspaceError>,
    },
    #[error("git returned invalid direct ref record `{record}`")]
    InvalidDirectRefRecord { record: String },
    #[error("failed to create clone target `{path}`: {source}")]
    CreateTarget {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "clone failed: {clone_error}; target `{path}` could not be cleaned up: {cleanup_source}"
    )]
    CleanupFailed {
        path: PathBuf,
        #[source]
        cleanup_source: io::Error,
        clone_error: Box<GitWorkspaceError>,
    },
    #[error("Git URL `{url}` does not contain a repository name")]
    RepositoryNameMissing { url: String },
    #[error("clone directory name `{name}` must be a single normal path component")]
    InvalidCloneDirectoryName { name: String },
    #[error("background Git operation `{operation}` failed: {message}")]
    BackgroundTaskFailed {
        operation: &'static str,
        message: String,
    },
}

/// 校验路径是 Git 主工作目录，并读取 remote URL 与 remote 默认分支。
pub fn validate_repository(path: &Path) -> Result<ValidatedRepository, GitWorkspaceError> {
    let selected = canonicalize(path)?;
    let root = output_path(
        &selected,
        "find repository root",
        &["rev-parse", "--show-toplevel"],
    )?;
    let root = canonicalize(&root)?;
    if selected != root {
        return Err(GitWorkspaceError::NotRepositoryRoot { selected, root });
    }

    let git_dir = output_path(
        &root,
        "find repository git directory",
        &["rev-parse", "--absolute-git-dir"],
    )?;
    let common_dir = output_path(
        &root,
        "find repository common directory",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let git_dir = canonicalize(&git_dir)?;
    let common_dir = canonicalize(&common_dir)?;
    if git_dir != common_dir {
        return Err(GitWorkspaceError::LinkedWorktree {
            git_dir,
            common_dir,
        });
    }

    let remote = primary_remote(&root)?;
    let remote_url = output_string(
        &root,
        "read repository remote URL",
        &["remote", "get-url", &remote],
    )?;
    let default_branch = default_branch(&root, &remote)?;

    Ok(ValidatedRepository {
        root,
        remote,
        remote_url,
        default_branch,
    })
}

/// 在后台线程执行 repository 校验，避免在 UI 调用线程运行 blocking Git。
pub async fn validate_repository_async(
    path: PathBuf,
) -> Result<ValidatedRepository, GitWorkspaceError> {
    spawn_git_task("validate repository", move || validate_repository(&path)).await
}

/// Clone repository 到明确目标路径；目标为 `None` 时使用 URL 中的 repository 名。
pub fn clone_repository(
    url: &str,
    target: Option<&Path>,
) -> Result<ValidatedRepository, GitWorkspaceError> {
    let derived_target;
    let target = match target {
        Some(target) => target,
        None => {
            derived_target = PathBuf::from(repository_name_from_url(url)?);
            &derived_target
        }
    };
    clone_to_target(url, target)
}

/// 在指定父目录 Clone repository，可选择覆盖 URL 推导出的目录名。
pub fn clone_repository_into(
    url: &str,
    parent: &Path,
    directory_name: Option<&str>,
) -> Result<ValidatedRepository, GitWorkspaceError> {
    let directory_name = match directory_name {
        Some(directory_name) => directory_name.to_string(),
        None => repository_name_from_url(url)?,
    };
    validate_clone_directory_name(&directory_name)?;
    clone_to_target(url, &parent.join(directory_name))
}

/// 在后台线程 Clone repository，避免长时间 Git 操作阻塞 UI。
pub async fn clone_repository_async(
    url: String,
    target: Option<PathBuf>,
) -> Result<ValidatedRepository, GitWorkspaceError> {
    spawn_git_task("clone repository", move || {
        clone_repository(&url, target.as_deref())
    })
    .await
}

/// 执行 fetch 后列出本地与远端完整分支引用。
pub fn fetch_and_list_refs(repo: &Path) -> Result<Vec<BranchRef>, GitWorkspaceError> {
    let remote = primary_remote(repo)?;
    git_output_for_operation(
        repo,
        "fetch repository refs",
        &["fetch", "--prune", "--quiet", "--no-tags", &remote],
    )?;
    list_branch_refs(repo)
}

/// 在后台线程 fetch 并列出分支引用，避免阻塞 UI。
pub async fn fetch_and_list_refs_async(repo: PathBuf) -> Result<Vec<BranchRef>, GitWorkspaceError> {
    spawn_git_task("fetch repository refs", move || fetch_and_list_refs(&repo)).await
}

/// 使用完整 refname 列出本地与远端分支。
pub fn list_branch_refs(repo: &Path) -> Result<Vec<BranchRef>, GitWorkspaceError> {
    let remotes = list_remotes(repo)?;
    let output = git_output_for_operation(
        repo,
        "list repository refs",
        &[
            "for-each-ref",
            "--format=%(refname)%09%(symref)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let stdout = String::from_utf8(output.stdout).map_err(|_| GitWorkspaceError::InvalidUtf8 {
        operation: "list repository refs",
    })?;

    parse_branch_ref_records(&stdout, &remotes)
}

pub(crate) fn parse_branch_ref_records(
    stdout: &str,
    remotes: &[String],
) -> Result<Vec<BranchRef>, GitWorkspaceError> {
    let mut refs = Vec::new();
    for record in stdout.lines() {
        let Some((full_ref, symref)) = record.split_once('\t') else {
            return Err(GitWorkspaceError::InvalidBranchRefRecord {
                record: record.to_string(),
            });
        };
        if full_ref.is_empty() || symref.contains('\t') {
            return Err(GitWorkspaceError::InvalidBranchRefRecord {
                record: record.to_string(),
            });
        }
        if !symref.is_empty() {
            continue;
        }
        refs.push(parse_branch_ref(full_ref, remotes)?);
    }

    Ok(refs)
}

/// 在后台线程列出分支引用，避免在 UI 调用线程运行 blocking Git。
pub async fn list_branch_refs_async(repo: PathBuf) -> Result<Vec<BranchRef>, GitWorkspaceError> {
    spawn_git_task("list repository refs", move || list_branch_refs(&repo)).await
}

/// 解析 remote HEAD，返回带完整 refname 的默认分支。
pub fn default_branch(repo: &Path, remote: &str) -> Result<BranchRef, GitWorkspaceError> {
    let symbolic_ref = format!("refs/remotes/{remote}/HEAD");
    let output = git_output_for_operation(
        repo,
        "read remote default branch",
        &["symbolic-ref", &symbolic_ref],
    );
    let full_ref = match output {
        Ok(output) => decode_stdout(output, "read remote default branch")?,
        Err(GitWorkspaceError::CommandFailed { stderr, .. }) => {
            return Err(GitWorkspaceError::DefaultBranchNotFound {
                repo: repo.to_path_buf(),
                remote: remote.to_string(),
                stderr,
            });
        }
        Err(error) => return Err(error),
    };
    let prefix = format!("refs/remotes/{remote}/");
    let Some(name) = full_ref.strip_prefix(&prefix) else {
        return Err(GitWorkspaceError::DefaultBranchNotFound {
            repo: repo.to_path_buf(),
            remote: remote.to_string(),
            stderr: format!("unexpected symbolic ref `{full_ref}`"),
        });
    };
    if name.is_empty() || name == "HEAD" {
        return Err(GitWorkspaceError::DefaultBranchNotFound {
            repo: repo.to_path_buf(),
            remote: remote.to_string(),
            stderr: format!("unexpected symbolic ref `{full_ref}`"),
        });
    }

    Ok(BranchRef::Remote {
        remote: remote.to_string(),
        name: name.to_string(),
        full_ref,
    })
}

/// 解析 `git worktree list --porcelain`，保留路径和完整 branch ref。
pub fn list_worktrees(repo: &Path) -> Result<Vec<WorktreeInfo>, GitWorkspaceError> {
    let output = git_output_for_operation(
        repo,
        "list repository worktrees",
        &["worktree", "list", "--porcelain", "-z"],
    )?;
    parse_worktrees(&output.stdout)
}

/// 在后台线程列出 worktree，避免在 UI 调用线程运行 blocking Git。
pub async fn list_worktrees_async(repo: PathBuf) -> Result<Vec<WorktreeInfo>, GitWorkspaceError> {
    spawn_git_task("list repository worktrees", move || list_worktrees(&repo)).await
}

/// 从完整 remote ref 创建不跟踪 upstream 的新本地分支和 linked worktree。
pub fn create_from_remote(
    repository: &Path,
    remote_ref: &str,
    new_branch: &str,
    worktree_path: &Path,
) -> Result<(), GitWorkspaceError> {
    create_from_remote_inner(
        repository,
        remote_ref,
        new_branch,
        worktree_path,
        || {},
        || {},
        || {},
    )
}

#[cfg(test)]
pub(crate) fn create_from_remote_with_hooks<BeforeCommand, AfterFailure>(
    repository: &Path,
    remote_ref: &str,
    new_branch: &str,
    worktree_path: &Path,
    before_command: BeforeCommand,
    after_failure: AfterFailure,
) -> Result<(), GitWorkspaceError>
where
    BeforeCommand: FnOnce(),
    AfterFailure: FnOnce(),
{
    create_from_remote_inner(
        repository,
        remote_ref,
        new_branch,
        worktree_path,
        before_command,
        after_failure,
        || {},
    )
}

#[cfg(test)]
pub(crate) fn create_from_remote_with_success_hook<AfterSuccess>(
    repository: &Path,
    remote_ref: &str,
    new_branch: &str,
    worktree_path: &Path,
    after_success: AfterSuccess,
) -> Result<(), GitWorkspaceError>
where
    AfterSuccess: FnOnce(),
{
    create_from_remote_inner(
        repository,
        remote_ref,
        new_branch,
        worktree_path,
        || {},
        || {},
        after_success,
    )
}

fn create_from_remote_inner<BeforeClaim, AfterFailure, AfterSuccess>(
    repository: &Path,
    remote_ref: &str,
    new_branch: &str,
    worktree_path: &Path,
    before_claim: BeforeClaim,
    after_failure: AfterFailure,
    after_success: AfterSuccess,
) -> Result<(), GitWorkspaceError>
where
    BeforeClaim: FnOnce(),
    AfterFailure: FnOnce(),
    AfterSuccess: FnOnce(),
{
    let expected_oid = validate_remote_ref(repository, remote_ref)?;
    validate_new_branch(repository, new_branch)?;
    validate_target_missing(worktree_path)?;
    before_claim();

    let full_ref = format!("refs/heads/{new_branch}");
    let zero_oid = "0".repeat(expected_oid.len());
    let claim_args = [
        "update-ref",
        "--no-deref",
        full_ref.as_str(),
        expected_oid.as_str(),
        zero_oid.as_str(),
    ];
    let claim_output = git_output_allow_failure_for_operation(
        repository,
        "atomically claim local branch",
        &claim_args,
    )
    .map_err(|claim_error| GitWorkspaceError::BranchClaimFailed {
        branch: new_branch.to_string(),
        expected_oid: expected_oid.clone(),
        claim_error: Box::new(claim_error),
    })?;
    if !claim_output.status.success() {
        return Err(GitWorkspaceError::BranchClaimFailed {
            branch: new_branch.to_string(),
            expected_oid: expected_oid.clone(),
            claim_error: Box::new(command_failed(
                "atomically claim local branch",
                &claim_args,
                &claim_output,
            )),
        });
    }

    let args = [
        OsStr::new("worktree"),
        OsStr::new("add"),
        worktree_path.as_os_str(),
        OsStr::new(new_branch),
    ];
    match git_output_with_os_args_for_operation(repository, "create worktree from remote", &args) {
        Ok(output) => {
            drop(output);
            after_success();
            verify_remote_worktree_creation(repository, worktree_path, new_branch, &expected_oid)
                .map_err(|verification_error| {
                    GitWorkspaceError::WorktreeCreationVerificationFailed {
                        worktree_path: worktree_path.to_path_buf(),
                        branch: new_branch.to_string(),
                        expected_oid,
                        verification_error: Box::new(verification_error),
                    }
                })
        }
        Err(create_error) => {
            after_failure();
            cleanup_failed_remote_creation(repository, new_branch, &expected_oid, create_error)
        }
    }
}

fn verify_remote_worktree_creation(
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
    expected_oid: &str,
) -> Result<(), GitWorkspaceError> {
    let registered_path = canonicalize(worktree_path)?;
    let mut matches = list_worktrees(repository)?
        .into_iter()
        .filter(|worktree| worktree.path == registered_path);
    let Some(worktree) = matches.next() else {
        return Err(GitWorkspaceError::WorktreeNotFound {
            path: registered_path,
        });
    };
    if matches.next().is_some() {
        return Err(GitWorkspaceError::AmbiguousWorktree {
            path: registered_path,
        });
    }

    let expected_branch = format!("refs/heads/{branch}");
    if worktree.is_bare
        || worktree.is_detached
        || worktree.branch.as_deref() != Some(&expected_branch)
    {
        return Err(GitWorkspaceError::WorktreeBranchMismatch {
            expected: expected_branch,
            actual: worktree
                .branch
                .unwrap_or_else(|| "<detached or missing>".to_string()),
        });
    }

    let actual_snapshot = direct_ref_snapshot(repository, &expected_branch)?;
    if actual_snapshot.direct_oid.as_deref() != Some(expected_oid)
        || actual_snapshot.symbolic_target.is_some()
    {
        return Err(GitWorkspaceError::BranchChanged {
            branch: branch.to_string(),
            expected_oid: expected_oid.to_string(),
            actual_oid: actual_snapshot.direct_oid,
            actual_symbolic_target: actual_snapshot.symbolic_target,
        });
    }
    Ok(())
}

/// 在后台线程从 remote ref 创建 linked worktree，避免阻塞 UI。
pub async fn create_from_remote_async(
    repository: PathBuf,
    remote_ref: String,
    new_branch: String,
    worktree_path: PathBuf,
) -> Result<(), GitWorkspaceError> {
    spawn_git_task("create worktree from remote", move || {
        create_from_remote(&repository, &remote_ref, &new_branch, &worktree_path)
    })
    .await
}

/// 从现有本地分支创建 linked worktree。
pub fn create_from_local(
    repository: &Path,
    local_branch: &str,
    worktree_path: &Path,
) -> Result<(), GitWorkspaceError> {
    let full_ref = format!("refs/heads/{local_branch}");
    validate_ref_exists(repository, &full_ref)?;
    if let Some(worktree) = list_worktrees(repository)?
        .into_iter()
        .find(|worktree| worktree.branch.as_deref() == Some(&full_ref))
    {
        return Err(GitWorkspaceError::BranchAlreadyCheckedOut {
            branch: local_branch.to_string(),
            path: worktree.path,
        });
    }
    validate_target_missing(worktree_path)?;

    let args = [
        OsStr::new("worktree"),
        OsStr::new("add"),
        worktree_path.as_os_str(),
        OsStr::new(local_branch),
    ];
    git_output_with_os_args_for_operation(repository, "create worktree from local branch", &args)?;
    Ok(())
}

/// 在后台线程从本地分支创建 linked worktree，避免阻塞 UI。
pub async fn create_from_local_async(
    repository: PathBuf,
    local_branch: String,
    worktree_path: PathBuf,
) -> Result<(), GitWorkspaceError> {
    spawn_git_task("create worktree from local branch", move || {
        create_from_local(&repository, &local_branch, &worktree_path)
    })
    .await
}

/// 只读校验 linked worktree 是否可删除。
///
/// 未请求删除分支时不读取 remote 或执行 merge 判断，`is_merged` 返回 `true`，
/// `merge_target` 返回空字符串。
pub fn deletion_preflight(
    repository: &Path,
    worktree_path: &Path,
    delete_branch: bool,
) -> Result<DeletionPreflight, GitWorkspaceError> {
    let worktree_path = canonicalize(worktree_path)?;
    let mut matches = list_worktrees(repository)?
        .into_iter()
        .filter(|worktree| worktree.path == worktree_path);
    let Some(worktree) = matches.next() else {
        return Err(GitWorkspaceError::WorktreeNotFound {
            path: worktree_path,
        });
    };
    if matches.next().is_some() {
        return Err(GitWorkspaceError::AmbiguousWorktree {
            path: worktree_path,
        });
    }
    if worktree.is_bare || worktree.is_detached {
        return Err(GitWorkspaceError::WorktreeHasNoLocalBranch {
            path: worktree_path,
        });
    }
    let Some(full_ref) = worktree.branch else {
        return Err(GitWorkspaceError::WorktreeHasNoLocalBranch {
            path: worktree_path,
        });
    };
    let Some(branch) = full_ref.strip_prefix("refs/heads/") else {
        return Err(GitWorkspaceError::WorktreeHasNoLocalBranch {
            path: worktree_path,
        });
    };
    if branch.is_empty() {
        return Err(GitWorkspaceError::WorktreeHasNoLocalBranch {
            path: worktree_path,
        });
    }
    let branch_snapshot = direct_ref_snapshot(repository, &full_ref)?;
    let Some(branch_oid) = branch_snapshot.direct_oid else {
        if branch_snapshot.symbolic_target.is_some() {
            return Err(GitWorkspaceError::WorktreeHasNoLocalBranch {
                path: worktree_path,
            });
        }
        return Err(GitWorkspaceError::BranchNotFound { full_ref });
    };

    let status = git_output_for_operation(
        &worktree_path,
        "check worktree status",
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !status.stdout.is_empty() {
        return Err(GitWorkspaceError::DirtyWorktree {
            path: worktree_path,
        });
    }

    if !delete_branch {
        return Ok(DeletionPreflight {
            worktree_path,
            branch: branch.to_string(),
            branch_oid,
            is_merged: true,
            merge_target: String::new(),
        });
    }

    let merge_target = match branch_upstream(repository, &full_ref)? {
        Some(upstream) => upstream,
        None => {
            let remote = primary_remote(repository)?;
            match default_branch(repository, &remote)? {
                BranchRef::Remote { full_ref, .. } => full_ref,
                BranchRef::Local { full_ref, .. } => {
                    return Err(GitWorkspaceError::InvalidRemoteRef { full_ref });
                }
            }
        }
    };
    let args = [
        "merge-base",
        "--is-ancestor",
        branch_oid.as_str(),
        merge_target.as_str(),
    ];
    let output =
        git_output_allow_failure_for_operation(repository, "check branch merge status", &args)?;
    let is_merged = match output.status.code() {
        Some(0) => true,
        Some(1) => false,
        Some(exit_code) => {
            debug_assert_ne!(exit_code, 0);
            debug_assert_ne!(exit_code, 1);
            return Err(command_failed("check branch merge status", &args, &output));
        }
        None => {
            return Err(command_failed("check branch merge status", &args, &output));
        }
    };

    Ok(DeletionPreflight {
        worktree_path,
        branch: branch.to_string(),
        branch_oid,
        is_merged,
        merge_target,
    })
}

/// 在后台线程执行 linked worktree 删除预检，避免阻塞 UI。
pub async fn deletion_preflight_async(
    repository: PathBuf,
    worktree_path: PathBuf,
    delete_branch: bool,
) -> Result<DeletionPreflight, GitWorkspaceError> {
    spawn_git_task("preflight worktree deletion", move || {
        deletion_preflight(&repository, &worktree_path, delete_branch)
    })
    .await
}

/// 删除已通过完整预检的 linked worktree，并按需删除对应本地分支。
pub fn remove_workspace(
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
    force_branch: bool,
) -> Result<(), GitWorkspaceError> {
    remove_workspace_with_runners(
        repository,
        worktree_path,
        branch,
        delete_branch,
        force_branch,
        || {},
        || {},
        run_branch_compare_delete,
        direct_ref_snapshot,
    )
}

#[cfg(test)]
pub(crate) fn remove_workspace_with_hook<F>(
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
    force_branch: bool,
    before_mutation: F,
) -> Result<(), GitWorkspaceError>
where
    F: FnOnce(),
{
    remove_workspace_with_hooks(
        repository,
        worktree_path,
        branch,
        delete_branch,
        force_branch,
        before_mutation,
        || {},
    )
}

#[cfg(test)]
pub(crate) fn remove_workspace_with_hooks<BeforeMutation, BeforeBranchDelete>(
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
    force_branch: bool,
    before_mutation: BeforeMutation,
    before_branch_delete: BeforeBranchDelete,
) -> Result<(), GitWorkspaceError>
where
    BeforeMutation: FnOnce(),
    BeforeBranchDelete: FnOnce(),
{
    remove_workspace_with_runners(
        repository,
        worktree_path,
        branch,
        delete_branch,
        force_branch,
        before_mutation,
        before_branch_delete,
        run_branch_compare_delete,
        direct_ref_snapshot,
    )
}

#[cfg(test)]
pub(crate) fn remove_workspace_with_delete_runner<DeleteRunner>(
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
    force_branch: bool,
    delete_runner: DeleteRunner,
) -> Result<(), GitWorkspaceError>
where
    DeleteRunner: FnOnce() -> Result<Output, GitWorkspaceError>,
{
    remove_workspace_with_runners(
        repository,
        worktree_path,
        branch,
        delete_branch,
        force_branch,
        || {},
        || {},
        move |_, _, _| delete_runner(),
        direct_ref_snapshot,
    )
}

#[cfg(test)]
pub(crate) fn remove_workspace_with_inspection_runner<DeleteRunner, InspectionRunner>(
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
    force_branch: bool,
    delete_runner: DeleteRunner,
    inspection_runner: InspectionRunner,
) -> Result<(), GitWorkspaceError>
where
    DeleteRunner: FnOnce() -> Result<Output, GitWorkspaceError>,
    InspectionRunner: FnOnce() -> Result<(), GitWorkspaceError>,
{
    remove_workspace_with_runners(
        repository,
        worktree_path,
        branch,
        delete_branch,
        force_branch,
        || {},
        || {},
        move |_, _, _| delete_runner(),
        move |repository, full_ref| {
            inspection_runner()?;
            direct_ref_snapshot(repository, full_ref)
        },
    )
}

fn remove_workspace_with_runners<
    BeforeMutation,
    BeforeBranchDelete,
    DeleteRunner,
    InspectionRunner,
>(
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
    force_branch: bool,
    before_mutation: BeforeMutation,
    before_branch_delete: BeforeBranchDelete,
    delete_runner: DeleteRunner,
    inspection_runner: InspectionRunner,
) -> Result<(), GitWorkspaceError>
where
    BeforeMutation: FnOnce(),
    BeforeBranchDelete: FnOnce(),
    DeleteRunner: FnOnce(&Path, &str, &str) -> Result<Output, GitWorkspaceError>,
    InspectionRunner: FnOnce(&Path, &str) -> Result<DirectRefSnapshot, GitWorkspaceError>,
{
    let preflight = deletion_preflight(repository, worktree_path, delete_branch)?;
    if preflight.branch != branch {
        return Err(GitWorkspaceError::WorktreeBranchMismatch {
            expected: branch.to_string(),
            actual: preflight.branch,
        });
    }
    if delete_branch && !preflight.is_merged && !force_branch {
        return Err(GitWorkspaceError::BranchNotMerged {
            branch: branch.to_string(),
            merge_target: preflight.merge_target,
        });
    }

    before_mutation();
    let full_ref = format!("refs/heads/{branch}");
    let actual_snapshot = direct_ref_snapshot(repository, &full_ref)?;
    if actual_snapshot.direct_oid.as_deref() != Some(&preflight.branch_oid)
        || actual_snapshot.symbolic_target.is_some()
    {
        return Err(GitWorkspaceError::BranchChanged {
            branch: branch.to_string(),
            expected_oid: preflight.branch_oid,
            actual_oid: actual_snapshot.direct_oid,
            actual_symbolic_target: actual_snapshot.symbolic_target,
        });
    }

    let remove_args = [
        OsStr::new("worktree"),
        OsStr::new("remove"),
        preflight.worktree_path.as_os_str(),
    ];
    git_output_with_os_args_for_operation(repository, "remove worktree", &remove_args)?;
    if delete_branch {
        before_branch_delete();
        let pre_delete_snapshot = match direct_ref_snapshot(repository, &full_ref) {
            Ok(snapshot) => snapshot,
            Err(inspection_error) => {
                return Err(GitWorkspaceError::BranchDeleteFailed {
                    worktree_path: preflight.worktree_path,
                    worktree_removed: true,
                    branch: branch.to_string(),
                    expected_oid: preflight.branch_oid.clone(),
                    actual_oid: None,
                    actual_symbolic_target: None,
                    delete_error: Box::new(GitWorkspaceError::BranchDeleteNotCompleted {
                        full_ref,
                        expected_oid: preflight.branch_oid,
                    }),
                    inspection_error: Some(Box::new(inspection_error)),
                });
            }
        };
        if pre_delete_snapshot.direct_oid.as_deref() != Some(&preflight.branch_oid)
            || pre_delete_snapshot.symbolic_target.is_some()
        {
            let actual_oid = pre_delete_snapshot.direct_oid;
            let actual_symbolic_target = pre_delete_snapshot.symbolic_target;
            return Err(GitWorkspaceError::BranchDeleteFailed {
                worktree_path: preflight.worktree_path,
                worktree_removed: true,
                branch: branch.to_string(),
                expected_oid: preflight.branch_oid.clone(),
                actual_oid: actual_oid.clone(),
                actual_symbolic_target: actual_symbolic_target.clone(),
                delete_error: Box::new(GitWorkspaceError::BranchChanged {
                    branch: branch.to_string(),
                    expected_oid: preflight.branch_oid,
                    actual_oid,
                    actual_symbolic_target,
                }),
                inspection_error: None,
            });
        }
        let delete_error = match delete_runner(repository, &full_ref, &preflight.branch_oid) {
            Ok(output) if output.status.success() => None,
            Ok(output) => Some(command_failed(
                "delete worktree branch at expected OID",
                &[
                    "update-ref",
                    "--no-deref",
                    "-d",
                    full_ref.as_str(),
                    preflight.branch_oid.as_str(),
                ],
                &output,
            )),
            Err(delete_error) => Some(delete_error),
        };
        let (actual_snapshot, inspection_error) = match inspection_runner(repository, &full_ref) {
            Ok(actual_snapshot) => (Some(actual_snapshot), None),
            Err(inspection_error) => (None, Some(Box::new(inspection_error))),
        };
        if delete_error.is_none()
            && inspection_error.is_none()
            && actual_snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.direct_oid.is_none() && snapshot.symbolic_target.is_none()
            })
        {
            return Ok(());
        }
        let delete_error =
            delete_error.unwrap_or_else(|| GitWorkspaceError::BranchDeleteNotCompleted {
                full_ref,
                expected_oid: preflight.branch_oid.clone(),
            });
        return Err(GitWorkspaceError::BranchDeleteFailed {
            worktree_path: preflight.worktree_path,
            worktree_removed: true,
            branch: branch.to_string(),
            expected_oid: preflight.branch_oid,
            actual_oid: actual_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.direct_oid.clone()),
            actual_symbolic_target: actual_snapshot.and_then(|snapshot| snapshot.symbolic_target),
            delete_error: Box::new(delete_error),
            inspection_error,
        });
    }
    Ok(())
}

fn run_branch_compare_delete(
    repository: &Path,
    full_ref: &str,
    expected_oid: &str,
) -> Result<Output, GitWorkspaceError> {
    git_output_allow_failure_for_operation(
        repository,
        "delete worktree branch at expected OID",
        &["update-ref", "--no-deref", "-d", full_ref, expected_oid],
    )
}

/// 在后台线程删除 linked worktree 和可选本地分支，避免阻塞 UI。
pub async fn remove_workspace_async(
    repository: PathBuf,
    worktree_path: PathBuf,
    branch: String,
    delete_branch: bool,
    force_branch: bool,
) -> Result<(), GitWorkspaceError> {
    spawn_git_task("remove worktree", move || {
        remove_workspace(
            &repository,
            &worktree_path,
            &branch,
            delete_branch,
            force_branch,
        )
    })
    .await
}

/// 从标准 Git URL、SCP 风格地址或本地路径解析 repository 名。
pub fn repository_name_from_url(url: &str) -> Result<String, GitWorkspaceError> {
    let trimmed = url.trim();
    let name = if is_windows_drive_absolute(trimmed) {
        repository_name_from_local_path(trimmed)
    } else {
        match url::Url::parse(trimmed) {
            Ok(parsed) => parsed
                .path_segments()
                .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
                .map(str::to_string),
            Err(_) => repository_name_from_local_path(trimmed),
        }
    }
    .map(|name| name.strip_suffix(".git").unwrap_or(&name).to_string())
    .filter(|name| !name.is_empty());

    name.ok_or_else(|| GitWorkspaceError::RepositoryNameMissing {
        url: url.to_string(),
    })
}

fn is_windows_drive_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn repository_name_from_local_path(path: &str) -> Option<String> {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\', ':'])
        .find(|segment| !segment.is_empty())
        .map(str::to_string)
}

fn validate_clone_directory_name(name: &str) -> Result<(), GitWorkspaceError> {
    let mut components = Path::new(name).components();
    let valid = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(component)), None) if component == OsStr::new(name)
    );
    if valid {
        Ok(())
    } else {
        Err(GitWorkspaceError::InvalidCloneDirectoryName {
            name: name.to_string(),
        })
    }
}

/// 将分支名转换为安全目录 slug，并追加 workspace ID 的前 8 位。
pub fn workspace_dir_name(branch: &str, workspace_id: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for character in branch.chars() {
        let safe = character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.');
        if safe {
            slug.push(character);
            last_was_separator = false;
        } else if !slug.is_empty() && !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "workspace" } else { slug };
    let short_id: String = workspace_id.chars().take(8).collect();
    if short_id.is_empty() {
        slug.to_string()
    } else {
        format!("{slug}-{short_id}")
    }
}

fn clone_to_target(url: &str, target: &Path) -> Result<ValidatedRepository, GitWorkspaceError> {
    if target.exists() {
        return Err(GitWorkspaceError::TargetExists {
            path: target.to_path_buf(),
        });
    }
    std::fs::create_dir(target).map_err(|source| GitWorkspaceError::CreateTarget {
        path: target.to_path_buf(),
        source,
    })?;

    let result = git_output_for_operation(target, "clone repository", &["clone", "--", url, "."])
        .and_then(|_| validate_repository(target));
    match result {
        Ok(repository) => Ok(repository),
        Err(clone_error) => {
            if let Err(cleanup_source) = std::fs::remove_dir_all(target) {
                return Err(GitWorkspaceError::CleanupFailed {
                    path: target.to_path_buf(),
                    cleanup_source,
                    clone_error: Box::new(clone_error),
                });
            }
            Err(clone_error)
        }
    }
}

fn primary_remote(repo: &Path) -> Result<String, GitWorkspaceError> {
    let remotes = list_remotes(repo)?;
    remotes
        .iter()
        .find(|remote| remote.as_str() == "origin")
        .or_else(|| remotes.first())
        .cloned()
        .ok_or_else(|| GitWorkspaceError::RemoteNotFound {
            repo: repo.to_path_buf(),
        })
}

fn list_remotes(repo: &Path) -> Result<Vec<String>, GitWorkspaceError> {
    let stdout = output_string(repo, "list repository remotes", &["remote"])?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty())
        .map(str::to_string)
        .collect())
}

fn validate_remote_ref(repository: &Path, remote_ref: &str) -> Result<String, GitWorkspaceError> {
    let remotes = list_remotes(repository)?;
    match parse_branch_ref(remote_ref, &remotes) {
        Ok(BranchRef::Remote { name, .. }) if name != "HEAD" => {}
        Ok(BranchRef::Local { .. })
        | Ok(BranchRef::Remote { .. })
        | Err(GitWorkspaceError::InvalidBranchRef { .. })
        | Err(GitWorkspaceError::AmbiguousRemoteRef { .. }) => {
            return Err(GitWorkspaceError::InvalidRemoteRef {
                full_ref: remote_ref.to_string(),
            });
        }
        Err(error) => return Err(error),
    }
    validate_ref_exists(repository, remote_ref)?;

    let output = git_output_allow_failure_for_operation(
        repository,
        "check whether remote ref is symbolic",
        &["symbolic-ref", "--quiet", remote_ref],
    )?;
    match output.status.code() {
        Some(0) => Err(GitWorkspaceError::InvalidRemoteRef {
            full_ref: remote_ref.to_string(),
        }),
        Some(1) => resolve_commit_oid(repository, remote_ref),
        Some(exit_code) => {
            debug_assert_ne!(exit_code, 0);
            debug_assert_ne!(exit_code, 1);
            Err(command_failed(
                "check whether remote ref is symbolic",
                &["symbolic-ref", "--quiet", remote_ref],
                &output,
            ))
        }
        None => Err(command_failed(
            "check whether remote ref is symbolic",
            &["symbolic-ref", "--quiet", remote_ref],
            &output,
        )),
    }
}

fn validate_new_branch(repository: &Path, branch: &str) -> Result<(), GitWorkspaceError> {
    let output = git_output_allow_failure_for_operation(
        repository,
        "validate new branch name",
        &["check-ref-format", "--branch", branch],
    )?;
    if !output.status.success() {
        return Err(GitWorkspaceError::InvalidBranchName {
            branch: branch.to_string(),
        });
    }

    let full_ref = format!("refs/heads/{branch}");
    if ref_exists(repository, &full_ref)? {
        return Err(GitWorkspaceError::BranchAlreadyExists {
            branch: branch.to_string(),
        });
    }
    Ok(())
}

fn validate_ref_exists(repository: &Path, full_ref: &str) -> Result<(), GitWorkspaceError> {
    if ref_exists(repository, full_ref)? {
        Ok(())
    } else {
        Err(GitWorkspaceError::BranchNotFound {
            full_ref: full_ref.to_string(),
        })
    }
}

fn ref_exists(repository: &Path, full_ref: &str) -> Result<bool, GitWorkspaceError> {
    let args = ["show-ref", "--verify", "--quiet", full_ref];
    let output =
        git_output_allow_failure_for_operation(repository, "check branch ref existence", &args)?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(exit_code) => {
            debug_assert_ne!(exit_code, 0);
            debug_assert_ne!(exit_code, 1);
            Err(command_failed("check branch ref existence", &args, &output))
        }
        None => Err(command_failed("check branch ref existence", &args, &output)),
    }
}

fn validate_target_missing(path: &Path) -> Result<(), GitWorkspaceError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            drop(metadata);
            Err(GitWorkspaceError::TargetExists {
                path: path.to_path_buf(),
            })
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GitWorkspaceError::TargetInspection {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn cleanup_failed_remote_creation(
    repository: &Path,
    branch: &str,
    expected_oid: &str,
    create_error: GitWorkspaceError,
) -> Result<(), GitWorkspaceError> {
    cleanup_failed_remote_creation_inner(repository, branch, expected_oid, create_error, || {})
}

#[cfg(test)]
pub(crate) fn cleanup_failed_remote_creation_with_hook<AfterDelete>(
    repository: &Path,
    branch: &str,
    expected_oid: &str,
    create_error: GitWorkspaceError,
    after_delete: AfterDelete,
) -> Result<(), GitWorkspaceError>
where
    AfterDelete: FnOnce(),
{
    cleanup_failed_remote_creation_inner(
        repository,
        branch,
        expected_oid,
        create_error,
        after_delete,
    )
}

fn cleanup_failed_remote_creation_inner<AfterDelete>(
    repository: &Path,
    branch: &str,
    expected_oid: &str,
    create_error: GitWorkspaceError,
    after_delete: AfterDelete,
) -> Result<(), GitWorkspaceError>
where
    AfterDelete: FnOnce(),
{
    let full_ref = format!("refs/heads/{branch}");
    let actual_snapshot = match direct_ref_snapshot(repository, &full_ref) {
        Ok(actual_snapshot) => actual_snapshot,
        Err(cleanup_error) => {
            return Err(GitWorkspaceError::WorktreeCreationCleanupFailed {
                branch: branch.to_string(),
                create_error: Box::new(create_error),
                cleanup_error: Box::new(cleanup_error),
            });
        }
    };
    if actual_snapshot.direct_oid.is_none() && actual_snapshot.symbolic_target.is_none() {
        return Err(create_error);
    }
    if actual_snapshot.direct_oid.as_deref() != Some(expected_oid)
        || actual_snapshot.symbolic_target.is_some()
    {
        return Err(GitWorkspaceError::WorktreeCreationBranchChanged {
            branch: branch.to_string(),
            expected_oid: expected_oid.to_string(),
            actual_oid: actual_snapshot.direct_oid,
            actual_symbolic_target: actual_snapshot.symbolic_target,
            create_error: Box::new(create_error),
        });
    }

    let args = [
        "update-ref",
        "--no-deref",
        "-d",
        full_ref.as_str(),
        expected_oid,
    ];
    let delete_error = match git_output_allow_failure_for_operation(
        repository,
        "clean up branch after worktree creation failure",
        &args,
    ) {
        Ok(cleanup_output) if cleanup_output.status.success() => None,
        Ok(cleanup_output) => Some(command_failed(
            "clean up branch after worktree creation failure",
            &args,
            &cleanup_output,
        )),
        Err(cleanup_error) => Some(cleanup_error),
    };
    after_delete();
    let (actual_snapshot, inspection_error) = match direct_ref_snapshot(repository, &full_ref) {
        Ok(actual_snapshot) => (Some(actual_snapshot), None),
        Err(inspection_error) => (None, Some(Box::new(inspection_error))),
    };
    if delete_error.is_none()
        && inspection_error.is_none()
        && actual_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.direct_oid.is_none() && snapshot.symbolic_target.is_none()
        })
    {
        return Err(create_error);
    }
    let delete_error =
        delete_error.unwrap_or_else(|| GitWorkspaceError::BranchDeleteNotCompleted {
            full_ref: full_ref.clone(),
            expected_oid: expected_oid.to_string(),
        });
    Err(GitWorkspaceError::WorktreeCreationCleanupFailed {
        branch: branch.to_string(),
        create_error: Box::new(create_error),
        cleanup_error: Box::new(GitWorkspaceError::BranchCleanupFailed {
            full_ref,
            expected_oid: expected_oid.to_string(),
            actual_oid: actual_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.direct_oid.clone()),
            actual_symbolic_target: actual_snapshot.and_then(|snapshot| snapshot.symbolic_target),
            delete_error: Box::new(delete_error),
            inspection_error,
        }),
    })
}

fn branch_upstream(repository: &Path, full_ref: &str) -> Result<Option<String>, GitWorkspaceError> {
    let upstream = output_string(
        repository,
        "read branch upstream",
        &["for-each-ref", "--format=%(upstream)", full_ref],
    )?;
    if upstream.is_empty() {
        Ok(None)
    } else {
        Ok(Some(upstream))
    }
}

fn resolve_commit_oid(repository: &Path, full_ref: &str) -> Result<String, GitWorkspaceError> {
    let commit_ref = format!("{full_ref}^{{commit}}");
    output_string(
        repository,
        "resolve branch commit OID",
        &["rev-parse", "--verify", &commit_ref],
    )
}

#[derive(Debug, Default)]
struct DirectRefSnapshot {
    direct_oid: Option<String>,
    symbolic_target: Option<String>,
}

fn direct_ref_snapshot(
    repository: &Path,
    full_ref: &str,
) -> Result<DirectRefSnapshot, GitWorkspaceError> {
    if let Some(symbolic_target) = symbolic_ref_target(repository, full_ref)? {
        return Ok(DirectRefSnapshot {
            direct_oid: None,
            symbolic_target: Some(symbolic_target),
        });
    }
    let stdout = output_string(
        repository,
        "read direct branch ref",
        &[
            "for-each-ref",
            "--format=%(refname)%09%(objectname)",
            full_ref,
        ],
    )?;
    let mut direct_oid = None;
    for record in stdout.lines() {
        let mut fields = record.split('\t');
        let refname = fields.next();
        let objectname = fields.next();
        let trailing = fields.next();
        let (Some(refname), Some(objectname), None) = (refname, objectname, trailing) else {
            return Err(GitWorkspaceError::InvalidDirectRefRecord {
                record: record.to_string(),
            });
        };
        if refname != full_ref {
            continue;
        }
        if direct_oid.is_some() || objectname.is_empty() {
            return Err(GitWorkspaceError::InvalidDirectRefRecord {
                record: record.to_string(),
            });
        }
        direct_oid = Some(objectname.to_string());
    }
    if let Some(symbolic_target) = symbolic_ref_target(repository, full_ref)? {
        return Ok(DirectRefSnapshot {
            direct_oid: None,
            symbolic_target: Some(symbolic_target),
        });
    }
    Ok(DirectRefSnapshot {
        direct_oid,
        symbolic_target: None,
    })
}

fn symbolic_ref_target(
    repository: &Path,
    full_ref: &str,
) -> Result<Option<String>, GitWorkspaceError> {
    let args = ["symbolic-ref", "--quiet", full_ref];
    let output =
        git_output_allow_failure_for_operation(repository, "inspect branch symbolic ref", &args)?;
    match output.status.code() {
        Some(0) => decode_stdout(output, "inspect branch symbolic ref").map(Some),
        Some(1) => Ok(None),
        Some(exit_code) => {
            debug_assert_ne!(exit_code, 0);
            debug_assert_ne!(exit_code, 1);
            Err(command_failed(
                "inspect branch symbolic ref",
                &args,
                &output,
            ))
        }
        None => Err(command_failed(
            "inspect branch symbolic ref",
            &args,
            &output,
        )),
    }
}

pub(crate) fn parse_branch_ref(
    full_ref: &str,
    remotes: &[String],
) -> Result<BranchRef, GitWorkspaceError> {
    if let Some(name) = full_ref.strip_prefix("refs/heads/") {
        if !name.is_empty() {
            return Ok(BranchRef::Local {
                name: name.to_string(),
                full_ref: full_ref.to_string(),
            });
        }
    }

    if let Some(remote_ref) = full_ref.strip_prefix("refs/remotes/") {
        let mut matches = Vec::new();
        for remote in remotes {
            let prefix = format!("{remote}/");
            if let Some(name) = remote_ref.strip_prefix(&prefix) {
                if !name.is_empty() {
                    matches.push((remote.clone(), name.to_string()));
                }
            }
        }
        return match matches.as_slice() {
            [(remote, name)] => Ok(BranchRef::Remote {
                remote: remote.clone(),
                name: name.clone(),
                full_ref: full_ref.to_string(),
            }),
            [] => Err(GitWorkspaceError::InvalidBranchRef {
                full_ref: full_ref.to_string(),
            }),
            matches => Err(GitWorkspaceError::AmbiguousRemoteRef {
                full_ref: full_ref.to_string(),
                remotes: matches.iter().map(|(remote, _)| remote.clone()).collect(),
            }),
        };
    }

    Err(GitWorkspaceError::InvalidBranchRef {
        full_ref: full_ref.to_string(),
    })
}

#[derive(Default)]
struct WorktreeBuilder {
    path: Option<PathBuf>,
    head: Option<String>,
    branch: Option<String>,
    is_bare: bool,
    is_detached: bool,
    is_locked: bool,
    locked_reason: Option<String>,
    is_prunable: bool,
    prunable_reason: Option<String>,
}

impl WorktreeBuilder {
    fn finish(self) -> Result<WorktreeInfo, GitWorkspaceError> {
        let path = self
            .path
            .ok_or_else(|| GitWorkspaceError::InvalidWorktreeRecord {
                record: "missing worktree path".to_string(),
            })?;
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(source) if self.is_prunable && source.kind() == io::ErrorKind::NotFound => {
                normalize_missing_path(&path)?
            }
            Err(source) => {
                return Err(GitWorkspaceError::Canonicalize { path, source });
            }
        };
        Ok(WorktreeInfo {
            path,
            head: self.head,
            branch: self.branch,
            is_bare: self.is_bare,
            is_detached: self.is_detached,
            is_locked: self.is_locked,
            locked_reason: self.locked_reason,
            is_prunable: self.is_prunable,
            prunable_reason: self.prunable_reason,
        })
    }
}

fn normalize_missing_path(path: &Path) -> Result<PathBuf, GitWorkspaceError> {
    let mut ancestor = path;
    let mut missing_components = Vec::new();
    while !ancestor.exists() {
        let Some(file_name) = ancestor.file_name() else {
            return canonicalize(path);
        };
        missing_components.push(file_name.to_os_string());
        let Some(parent) = ancestor.parent() else {
            return canonicalize(path);
        };
        ancestor = parent;
    }

    let mut normalized = canonicalize(ancestor)?;
    for component in missing_components.into_iter().rev() {
        normalized.push(component);
    }
    Ok(normalized)
}

pub(crate) fn parse_worktrees(stdout: &[u8]) -> Result<Vec<WorktreeInfo>, GitWorkspaceError> {
    let mut worktrees = Vec::new();
    let mut current = WorktreeBuilder::default();
    let mut has_record = false;

    for field in stdout.split(|byte| *byte == 0) {
        if field.is_empty() {
            if has_record {
                worktrees.push(current.finish()?);
                current = WorktreeBuilder::default();
                has_record = false;
            }
            continue;
        }
        has_record = true;
        if let Some(path) = field.strip_prefix(b"worktree ") {
            current.path = Some(path_from_git_bytes(path)?);
        } else if let Some(head) = field.strip_prefix(b"HEAD ") {
            current.head = Some(decode_worktree_text(head)?);
        } else if let Some(branch) = field.strip_prefix(b"branch ") {
            current.branch = Some(decode_worktree_text(branch)?);
        } else if field == b"bare" {
            current.is_bare = true;
        } else if field == b"detached" {
            current.is_detached = true;
        } else if field == b"locked" {
            current.is_locked = true;
        } else if let Some(reason) = field.strip_prefix(b"locked ") {
            current.is_locked = true;
            current.locked_reason = Some(decode_worktree_text(reason)?);
        } else if field == b"prunable" {
            current.is_prunable = true;
        } else if let Some(reason) = field.strip_prefix(b"prunable ") {
            current.is_prunable = true;
            current.prunable_reason = Some(decode_worktree_text(reason)?);
        } else {
            return Err(GitWorkspaceError::InvalidWorktreeRecord {
                record: format!("{field:?}"),
            });
        }
    }
    if has_record {
        worktrees.push(current.finish()?);
    }
    Ok(worktrees)
}

fn decode_worktree_text(field: &[u8]) -> Result<String, GitWorkspaceError> {
    String::from_utf8(field.to_vec()).map_err(|_| GitWorkspaceError::InvalidWorktreeRecord {
        record: format!("non-UTF-8 text field {field:?}"),
    })
}

#[cfg(unix)]
fn path_from_git_bytes(path: &[u8]) -> Result<PathBuf, GitWorkspaceError> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(path.to_vec())))
}

#[cfg(not(unix))]
fn path_from_git_bytes(path: &[u8]) -> Result<PathBuf, GitWorkspaceError> {
    String::from_utf8(path.to_vec())
        .map(PathBuf::from)
        .map_err(|_| GitWorkspaceError::InvalidWorktreeRecord {
            record: format!("non-UTF-8 worktree path {path:?}"),
        })
}

fn canonicalize(path: &Path) -> Result<PathBuf, GitWorkspaceError> {
    path.canonicalize()
        .map_err(|source| GitWorkspaceError::Canonicalize {
            path: path.to_path_buf(),
            source,
        })
}

fn output_path(
    repo: &Path,
    operation: &'static str,
    args: &[&str],
) -> Result<PathBuf, GitWorkspaceError> {
    let mut path_args = vec!["-c", "core.quotePath=false"];
    path_args.extend_from_slice(args);
    let output = git_output_for_operation(repo, operation, &path_args)?;
    decode_git_path_output(&output.stdout, operation)
}

pub(crate) fn decode_git_path_output(
    stdout: &[u8],
    operation: &'static str,
) -> Result<PathBuf, GitWorkspaceError> {
    #[cfg(unix)]
    let path = stdout.strip_suffix(b"\n").unwrap_or(stdout);
    #[cfg(not(unix))]
    let path = stdout
        .strip_suffix(b"\r\n")
        .or_else(|| stdout.strip_suffix(b"\n"))
        .unwrap_or(stdout);
    path_from_git_bytes(path).map_err(|error| match error {
        GitWorkspaceError::InvalidWorktreeRecord { .. } => {
            GitWorkspaceError::InvalidUtf8 { operation }
        }
        error => error,
    })
}

fn output_string(
    repo: &Path,
    operation: &'static str,
    args: &[&str],
) -> Result<String, GitWorkspaceError> {
    let output = git_output_for_operation(repo, operation, args)?;
    decode_stdout(output, operation)
}

fn decode_stdout(output: Output, operation: &'static str) -> Result<String, GitWorkspaceError> {
    String::from_utf8(output.stdout)
        .map(|stdout| stdout.trim().to_string())
        .map_err(|_| GitWorkspaceError::InvalidUtf8 { operation })
}

fn git_output(repo: &Path, args: &[&str]) -> Result<Output, GitWorkspaceError> {
    git_output_for_operation(repo, "run git command", args)
}

fn git_output_for_operation(
    repo: &Path,
    operation: &'static str,
    args: &[&str],
) -> Result<Output, GitWorkspaceError> {
    let display_args = args.iter().map(|arg| (*arg).to_string()).collect();
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    git_output_with_display_args_for_operation(repo, operation, &os_args, display_args)
}

pub(crate) fn git_output_with_os_args_for_operation(
    repo: &Path,
    operation: &'static str,
    args: &[&OsStr],
) -> Result<Output, GitWorkspaceError> {
    let display_args = args.iter().map(|arg| format!("{arg:?}")).collect();
    git_output_with_display_args_for_operation(repo, operation, args, display_args)
}

fn git_output_with_display_args_for_operation(
    repo: &Path,
    operation: &'static str,
    args: &[&OsStr],
    display_args: Vec<String>,
) -> Result<Output, GitWorkspaceError> {
    let output = execute_git(repo, operation, args, display_args.clone())?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(GitWorkspaceError::CommandFailed {
            operation,
            args: display_args,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

fn git_output_allow_failure_for_operation(
    repo: &Path,
    operation: &'static str,
    args: &[&str],
) -> Result<Output, GitWorkspaceError> {
    let display_args = args.iter().map(|arg| (*arg).to_string()).collect();
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    execute_git(repo, operation, &os_args, display_args)
}

fn execute_git(
    repo: &Path,
    operation: &'static str,
    args: &[&OsStr],
    display_args: Vec<String>,
) -> Result<Output, GitWorkspaceError> {
    let output = command::blocking::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args.iter().copied())
        .output()
        .map_err(|source| GitWorkspaceError::CommandIo {
            operation,
            args: display_args,
            source,
        })?;
    Ok(output)
}

fn command_failed(operation: &'static str, args: &[&str], output: &Output) -> GitWorkspaceError {
    GitWorkspaceError::CommandFailed {
        operation,
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    }
}

async fn spawn_git_task<T, F>(operation: &'static str, task: F) -> Result<T, GitWorkspaceError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, GitWorkspaceError> + Send + 'static,
{
    tokio::task::spawn_blocking(task).await.map_err(|error| {
        GitWorkspaceError::BackgroundTaskFailed {
            operation,
            message: error.to_string(),
        }
    })?
}
