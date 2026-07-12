# Worktree Git Ownership 与安全删除设计

## 背景

Repository Workspaces 的 Task 5 已实现 worktree 创建、删除 preflight 和分支删除，但质量审查发现以下并发安全问题：

1. 仅比较 ref 名称和 commit OID 无法识别同 OID 的 delete/recreate ABA，创建失败 cleanup 或 workspace 删除可能误删其他操作重新创建的 branch。
2. 安全删除只保存 merge target 名称。upstream 或默认分支在 preflight 后后退时，旧的 merged 结论会失效。
3. 目标路径的 `symlink_metadata` 检查与 `git worktree add` 之间仍有 TOCTOU；Git 会接受并接管并发创建的空目录。
4. Remote worktree 创建成功后的验证没有检查 upstream 仍为空。
5. 部分包装错误没有通过 `std::error::Error::source()` 暴露主底层错误。

本设计优先保证不误删用户数据。无法证明 branch ownership 时，保留残留并返回结构化错误，不以自动清理为名执行推测性删除。

## 目标

- 关闭同 OID ABA 导致的 branch 误删路径。
- 删除前验证 branch generation、branch OID 和 merge target OID，并在 mutation 期间锁定相关 refs。
- 原子 claim worktree 目标目录，避免接管并发创建的空目录。
- 创建成功后验证 worktree 注册状态、attached branch、branch OID 和 upstream 后置条件。
- 对无法自动补偿的残留状态提供完整、可操作、可追踪的结构化错误。
- 保持所有 Git 参数独立传递，Path 使用 `OsStr`，blocking Git 继续由 async wrapper 调度到后台线程。

## 非目标

- 不在 Git 服务中实现数据库、Workspace UI 或页签补偿事务。
- 不尝试让多个 Git/文件系统操作成为真正的跨资源原子事务。
- 不在无法证明 ownership 时猜测 branch 是否由本次操作创建。
- 不为外部进程直接篡改 `.git` 内部文件提供安全保证；支持范围是标准 Git 命令产生的状态变化。

## 方案比较

### 方案 A：残留优先创建 + 持锁删除事务

创建失败时不自动删除无法证明 ownership 的 branch。删除时使用 reflog generation token，并通过 prepared `git update-ref --stdin` transaction 锁定 branch 和 merge target，跨越 `git worktree remove`。

优点：关闭数据误删路径，错误状态诚实，符合 Fail-Fast。缺点：实现和测试复杂，需要处理 Git transaction 协议与部分 mutation。

### 方案 B：继续使用 OID compare-delete

保留现有 `update-ref --no-deref -d <ref> <expected-oid>`。

优点：简单。缺点：同 OID ABA 无法被检测，不能满足安全要求。

### 方案 C：永不自动删除 branch

创建失败和 workspace 删除都只移除 worktree，始终保留 branch。

优点：数据安全边界最简单。缺点：实质取消“同时删除本地分支”，产品行为退化。

采用方案 A。

## 创建流程

### 目标目录 claim

1. 完成 repository、remote ref、branch name 和 branch 不存在等只读校验。
2. 使用 `std::fs::create_dir(worktree_path)` 原子 claim 目标路径。
3. `AlreadyExists` 返回 `TargetExists`；其他 I/O 错误返回结构化 claim 错误。
4. claim 成功后记录该目录由本次调用创建。

并发方在 claim 后调用 `create_dir` 会得到 `AlreadyExists`。本设计不把“同一用户主动删除并替换已 claim 目录”视为普通并发场景；创建完成后仍会重新验证 canonical registered path，发现身份或注册状态异常时返回残留错误。

### Git 创建

目标目录已由本次调用 claim 后，执行：

```text
git -C <repository> worktree add --no-track -b <new-branch> <claimed-path> <remote-ref>
```

branch 由同一个 Git 命令创建，不再在命令前创建最终 local ref。若命令因并发 branch、Git I/O 或其他原因失败，Git 服务不自动删除可能存在的 branch，因为 ref 名称和 OID 无法证明其 generation ownership。

### 创建后验证

Git 命令成功后验证：

1. `list_worktrees` 中仅有一个 canonical registered path 与目标一致。
2. worktree 为 attached、非 bare，并指向 `refs/heads/<new-branch>`。
3. local branch 为 direct ref，OID 等于 remote ref 创建前解析的 expected OID。
4. local branch upstream 为空。

任一验证失败时返回 `WorktreeCreationVerificationFailed`。错误包含 worktree path、branch、expected OID、实际 direct/symbolic 状态和 upstream。该路径不删除无法证明 ownership 的 branch 或 worktree，而是明确报告残留，交由后续模型补偿与启动一致性检查处理。

### 创建失败目录清理

Git 命令失败后：

1. 检查目标是否已注册为 worktree；已注册时不删除目录。
2. 未注册且目录仍为空时，删除本次 claim 的目录。
3. 目录不为空、类型变化或检查失败时保留目录，并在错误中记录 cleanup failure。
4. 无论目录是否清理，均不自动删除可能残留的 local branch。

`WorktreeCreationFailed` 必须保留原始 Git stderr/IO 错误，并显式提供 `branch_may_remain`、`worktree_registered`、`claimed_directory_removed` 等残留状态。

## 删除 preflight

`DeletionPreflight` 在既有字段基础上保存：

- canonical registered worktree path；
- branch full ref 与 branch OID；
- merge target full ref 与 merge target OID；
- branch reflog generation token；
- dirty、attached 和 merge 结论。

### Reflog generation token

1. 使用 `git rev-parse --git-path logs/refs/heads/<branch>` 获取 reflog 路径。
2. 读取完整 reflog bytes，并使用仓库已有 `sha2` 依赖计算 SHA-256。
3. reflog 缺失、不可读或不是普通文件时，`delete_branch=true` 的 preflight 失败；`delete_branch=false` 不需要 token。
4. token 代表 preflight 时 Git-mediated branch generation。标准 Git delete/recreate 或 update 会改变 reflog 内容。

### Merge target

- 有 upstream 时保存 upstream full ref 与解析后的 OID。
- 无 upstream 时保存 primary remote default branch full ref 与 OID。
- `merge-base --is-ancestor <branch-oid> <merge-target-oid>` 使用固定 OID，而不是后续可变 ref 名称。

## 持锁删除事务

branch 删除使用 `git update-ref --stdin` 子进程。命令 stdin/stdout/stderr 均通过 `crates/command` 管理。

### Transaction 准备

向 transaction 发送等价命令：

```text
start
verify <branch-ref> <branch-oid>
verify <merge-target-ref> <merge-target-oid>
delete <branch-ref> <branch-oid>
prepare
```

force delete 不需要验证 merge target，但仍验证 branch ref/OID 和 reflog generation。

`prepare` 成功后 Git 持有相关 ref locks，pending delete 尚未提交。实现必须解析每个协议响应；未知、缺失或失败响应均 abort 并返回结构化 transaction 错误。

### 锁内验证与 mutation

1. transaction prepared 后重新计算 branch reflog generation token。
2. token 不匹配时发送 `abort`，等待子进程退出，并在任何 mutation 前返回 `BranchGenerationChanged`。
3. token 匹配后执行 `git worktree remove <canonical-registered-path>`。
4. worktree remove 失败时发送 `abort`，branch 保留。
5. worktree remove 成功后发送 `commit`，提交 branch delete。

实现前必须通过真实 Git prototype 验证：prepared branch/merge-target locks 不会阻止 `git worktree remove`。若 prototype 不成立，停止实现并重新设计，不回退到 OID-only delete。

### 部分 mutation

worktree remove 成功但 transaction commit、响应读取或子进程等待失败时，返回 `BranchDeleteTransactionFailed`，明确包含：

- canonical worktree path；
- `worktree_removed: true`；
- branch/merge-target ref 与 expected OID；
- transaction 阶段；
- Git stderr/IO source；
- branch 当前状态检查结果或检查错误。

不得把该状态包装成普通 command error。

## Target 目录身份与清理

目标目录通过原子 `create_dir` claim。创建命令前和创建后均验证该路径仍为目录；Git 成功后以 canonical registered path 作为最终身份。

失败清理只删除仍为空且未注册的 claimed 目录。目录中出现任何内容时不递归删除，避免清理并发方或用户写入的数据。

## 错误链

- 单一主底层错误字段使用 `#[source]`。
- 同时存在 operation error 和 cleanup/inspection error 时，operation error 为主 source；次级错误保留在字段和 Display 中。
- transaction protocol、abort、commit、wait 和 post-failure inspection 分别保留阶段信息。
- 所有用户可见错误保留 Git stderr 中的关键原因。

## 测试设计

### 创建

- claim 前目标不存在，claim 后并发 `create_dir` 返回 `AlreadyExists`。
- 并发创建空目标目录不能被 Git 静默接管。
- Git 创建失败时可能残留 branch，错误明确报告且不自动删除 branch。
- 成功后 hook 设置 upstream，verification 返回结构化残留错误。
- registered path、attached branch、direct OID 和 upstream none 的正常成功路径。
- claimed 目录仅在未注册且为空时清理；非空目录保留。

### 删除

- 同 OID delete/recreate 改变 reflog token，在 transaction prepare 后、worktree remove 前被拒绝。
- merge target 在 preflight 后后退，transaction `verify`/`prepare` 失败且无 mutation。
- prepared transaction 下 worktree remove 成功，commit 后 branch 消失。
- worktree remove 失败触发 abort，branch 保留。
- commit/IO/协议失败明确报告部分 mutation。
- reflog 缺失时 delete-branch preflight 失败；keep-branch 删除仍可执行。
- force delete 跳过 merge target verify，但不跳过 branch generation/OID 验证。

### 错误链

- 对主包装错误调用 `source()` 能获得底层 CommandFailed/CommandIo/transaction error。
- 次级 cleanup/inspection 错误仍出现在 Display 和结构化字段中。

## 风险与约束

- prepared transaction 是实现前的强制 prototype gate。
- reflog generation 仅覆盖标准 Git 命令产生的 ref 变化；直接编辑 `.git` 文件不在支持范围。
- reflog token 读取与哈希会增加删除 preflight 成本，但 branch 删除是低频操作，成本可接受。
- `git.rs` 已较大。实现计划应优先把 transaction protocol 和 reflog generation 拆为职责单一的私有单元；不做与 Task 5 无关的重构。

## 规格同步

实现时需要同步更新 `specs/repository-workspaces/TECH.md`：

- 创建失败无法证明 branch ownership 时保留 branch 并报告残留；
- 目标目录通过原子 claim；
- 删除使用 reflog generation 与 prepared ref transaction；
- merge target 以 ref + OID 固定并在 transaction 中验证。
