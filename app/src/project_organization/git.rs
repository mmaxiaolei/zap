use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    process::Output,
};

use thiserror::Error;

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
    #[error("git returned invalid worktree record: {record}")]
    InvalidWorktreeRecord { record: String },
    #[error("clone target `{path}` already exists")]
    TargetExists { path: PathBuf },
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
    git_output_for_operation(
        repo,
        "fetch repository refs",
        &["fetch", "--prune", "--quiet", "--no-tags"],
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
    let mut remotes_for_matching = remotes.clone();
    remotes_for_matching.sort_by_key(|remote| std::cmp::Reverse(remote.len()));
    let stdout = output_string(
        repo,
        "list repository refs",
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;

    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .filter(|full_ref| !is_remote_head_ref(full_ref, &remotes))
        .map(|full_ref| parse_branch_ref(full_ref, &remotes_for_matching))
        .collect()
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

fn parse_branch_ref(full_ref: &str, remotes: &[String]) -> Result<BranchRef, GitWorkspaceError> {
    if let Some(name) = full_ref.strip_prefix("refs/heads/") {
        if !name.is_empty() {
            return Ok(BranchRef::Local {
                name: name.to_string(),
                full_ref: full_ref.to_string(),
            });
        }
    }

    if let Some(remote_ref) = full_ref.strip_prefix("refs/remotes/") {
        for remote in remotes {
            let prefix = format!("{remote}/");
            if let Some(name) = remote_ref.strip_prefix(&prefix) {
                if !name.is_empty() {
                    return Ok(BranchRef::Remote {
                        remote: remote.clone(),
                        name: name.to_string(),
                        full_ref: full_ref.to_string(),
                    });
                }
            }
        }
    }

    Err(GitWorkspaceError::InvalidBranchRef {
        full_ref: full_ref.to_string(),
    })
}

fn is_remote_head_ref(full_ref: &str, remotes: &[String]) -> bool {
    remotes
        .iter()
        .any(|remote| full_ref == format!("refs/remotes/{remote}/HEAD"))
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
    output_string(repo, operation, args).map(PathBuf::from)
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
    let output = command::blocking::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args.iter().map(OsStr::new))
        .output()
        .map_err(|source| GitWorkspaceError::CommandIo {
            operation,
            args: display_args,
            source,
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(GitWorkspaceError::CommandFailed {
            operation,
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
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
