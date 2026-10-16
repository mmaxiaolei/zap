# Workspace Agent 状态 UI 优化（参考 Orca）

> 分支：`feat/repository-workspaces`  
> 对照：Orca Desktop（图1 / [onorca.dev](https://www.onorca.dev/)）vs 当前 Zap 树（图2）  
> 增量于：`2026-08-26-workspace-agent-activity-design.md`、`2026-08-31-workspace-tabs-in-tree-design.md`

## 1. 问题诊断

当前树是 `repository → workspace → tab`，agent 活动已下沉到页签子节点（品牌头像 + 呼吸/黄环）。图2里仍不清楚，根因不是「没有状态」，而是 **agent 与普通终端在视觉上同构**：

| 维度 | 当前 Zap（图2） | Orca（图1） |
|------|----------------|-------------|
| 信息架构 | 所有页签平铺；agent / shell 同一行样式 | workspace 下有明确的 **Agents** 子区（如 `3 agents`） |
| 身份 | Idle 时回退 `$` 提示符图标；Running 才换品牌头像 | **始终**用品牌圆标（Grok / Claude / …） |
| 状态语义 | 靠绿点 / 呼吸环，无文字；Done 后头像立刻消失 | 右侧明文 **Idle / Done / Running…** + 相对时间 `now` / `3h` |
| 标题 | 常为命令名、会话短名（`lg`、`mvn clean…`） | 标题是 **agent 名**；任务内容在中间面板 |
| 扫描成本 | 要区分「哪个是 AI、是否在跑」需猜图标变化 | 一眼：谁在跑、谁闲着、多久没动 |

现有设计文档刻意做了几件「正确但偏克制」的事，也正好造成了图2的观感：

- Success / Error / Cancelled **不残留头像**（`WorkspaceAgentPhase` 只含 InProgress / Blocked）。
- 折叠 workspace 父行 **不显示具体 agent**，只绿点 + 页签数。
- 页签 Idle 槽用 `ITERM_PROMPT_ICON`（`$`），agent 页签闲置时与普通终端无差。
- 多 agent **不叠头像、不做 +N**（父行仍禁止）。

## 2. 设计目标

1. **一眼认出 AI**：凡是 agent 会话页签，左侧永远是品牌圆标，不再落回 `$`。
2. **状态可读**：环动画保留给「正在跑 / 阻塞」；补充短文案状态 + 相对时间，对齐 Orca 扫描习惯。
3. **层级可读**：在 workspace 下把 agent 页签与普通终端页签在视觉上分区（不必改数据模型）。
4. **不破坏现有树交互**：点选、关闭、拖拽排序、`+` 新建、信息栏语义保持。

## 3. 非目标（首期）

- 不做 Orca 的 worktree 卡片大框、手机 companion、并行 ADE 全套。
- 不把 tab count 改成 agent 数（右侧数字仍是页签数）。
- 不在折叠父行叠多个头像（可后续做「折叠时显示最多 3 个小圆标」为可选增强）。
- 不按 Oz 底层模型换品牌（仍按 CLIAgent / Oz 身份）。

## 4. 推荐信息架构（Orca 映射到 Zap）

```text
repository
  └── workspace                    [折叠时：绿点 | 页签数]
        ├── Agents · 3             ← 可选分组头（见 Phase B）
        │     ├── [Grok圆标]  Grok          Idle · now
        │     ├── [dip圆标]   dip-agent     Idle · now
        │     └── [Claude圆标] Claude       Done · 3h
        └── Terminals              ← 普通终端仍用 $ / 类型图标
              ├── [绿点|$]  mvn clean package…
              └── [$]       Terminal 2
```

映射关系：

- Orca 的「分支 / worktree 行」→ Zap 的 **workspace 行**。
- Orca 的「Agents 列表」→ Zap workspace 下 **带 agent 身份的页签**（不必新建实体表）。
- Orca 中间主区 → Zap 现有页签内容（终端 / agent TUI），不动。

## 5. 页签行视觉规格（核心）

### 5.1 行布局

```text
[indent][16px 身份槽][ 主标题 …… ][ 状态文案 · 相对时间 ][hover 关闭]
```

- 身份槽宽仍锁在现有 `WORKSPACE_ACTIVITY_SLOT_SIZE`（16px），避免树左右跳。
- 右侧状态区用 theme `sub_text`，字号可略小于 body；窄宽时优先截断主标题，再藏相对时间，**最后**藏状态词。
- 选中高亮仍在活动页签子节点（现有规则）。

### 5.2 身份槽规则（改 Idle 回退）

| 页签类型 | Idle | InProgress | Blocked | Done / Error / Cancelled |
|----------|------|------------|---------|---------------------------|
| CLI / Oz agent | **品牌圆标，无环** | 品牌圆标 + 品牌色呼吸环 | 品牌圆标 + 静态黄环 | **品牌圆标，无环**（残留，见 5.4） |
| 普通终端 | `$` / 类型图标 | — | — | — |
| 普通终端 + shell 长任务 | — | 绿点 | — | 结束后回 `$` |

要点：取消「agent 结束后立刻空成 `$`」；agent 页签的类型图标永不回到 iterm `$`。

### 5.3 状态文案

| 内部状态 | 文案（EN / 跟 UI 语言） | 环 |
|----------|-------------------------|----|
| InProgress | Running / 运行中 | 呼吸 |
| Blocked | Needs input / 待确认 | 黄静态 |
| Success（保留期） | Done / 完成 | 无 |
| Error | Failed / 失败 | 无（可用危险色字） |
| Cancelled | Cancelled / 已取消 | 无 |
| agent 闲置（无会话或 Success 过保留期后仍挂着页签） | Idle / 空闲 | 无 |

相对时间：相对「进入当前状态」的时刻；不足 1 分钟显示 `now`；之后 `3m` / `2h` / `1d`。数据侧需为会话补一个 `phase_changed_at`（CLI / Oz 各接现有 status 变更点）。

### 5.4 Done 残留策略

现设计「结束立刻消失」适合「活动灯」，不适合「agent 列表」。改为：

- Success / Error / Cancelled：**保留品牌圆标 + 文案**，直到用户关页签，或超过 **N 小时**（建议默认 24h，可配置）后降级为 Idle（仍保留品牌圆标，去掉 Done）。
- 聚合「有活动」进折叠绿点时：**只计 InProgress / Blocked**，不计 Done 残留，避免侧栏常亮。

## 6. 分组头（Phase B，强烈建议但可第二刀）

在同一 workspace 的 `tab_nodes` 渲染时：

1. 先渲染 `Agents · k` 轻量分组头（不可选中，或点击仅展开/折叠该组）。
2. 再渲染 agent 页签（按最近活动时间倒序，与 Orca 扫描一致）。
3. 再渲染 `Terminals` 分组头 + 普通终端页签（保持现有标题规则：命令名 / Terminal N）。

判定「是 agent 页签」：能解析出 `WorkspaceAgentIdentity`（含已结束但仍绑定 CLI/Oz 会话的页签），与 `tab_node_activity` 同源。

无 agent 页签时不显示 Agents 分组头，避免空壳。

## 7. Workspace 父行（折叠时）

保持现有：绿点（任一子页签 InProgress/Blocked 或 shell 长任务）+ 页签数。  
可选增强（Phase C）：折叠时在名称左侧用最多 3 个 10px 品牌圆标并排，超出 `+N`——这才是 Orca「多 agent 一眼」在折叠态的补丁；展开后仍以子列表为准。

展开父行继续 **不** 画具体 agent 头像（与 `workspace_parent_activity_slot` 一致）。

## 8. 数据与代码落点

| 改动 | 位置 |
|------|------|
| 扩展 phase：`Done` / `Failed` / `Cancelled` / `Idle`（或并行 `TabAgentPresentation`） | `workspace_agent_activity.rs` |
| Idle agent 不再映射到 `TabNodeActivity::Idle` 的 `$` 图标 | `project_tree_tab.rs` + `render_tab_activity_slot` |
| 行右侧状态 + 相对时间 | `render_tab_row` in `view/project_tree.rs` |
| `phase_changed_at` / 相对时间格式化 | Workspace 聚合 → `ProjectTreeTabNode` 新字段 |
| 分组渲染顺序 | `ProjectTreePanel` 渲染 workspace 子节点循环 |
| 文案 i18n | 现有 UI 字符串管线 |
| 单测 | `workspace_agent_activity_tests`、`project_tree_tab_tests`、`project_tree_tests` |

不改：页签归属表结构、worktree 生命周期、信息栏、拖拽规则（分组头不参与排序；仅 agent/terminal 子节点可拖，跨组拖第一期禁止或自动改归属语义需另开设计）。

## 9. 分阶段落地

### Phase A — 可读性（建议先做，对图2痛点命中最高）

1. Agent 页签 Idle/Done：**品牌圆标常驻**，去掉 `$` 回退。  
2. 右侧增加状态词 + 相对时间。  
3. Done/Failed 残留规则 + 折叠绿点不计 Done。  
4. 标题优先显示 **agent 显示名**（Claude / Grok / …）；会话短名或最近命令可作为次级 tooltip，避免 `lg` 这种不可读主标题。

验收：图2里 `grok` / `lg` 行一眼是 AI；Running 有环+「运行中」；结束后仍是品牌标+「完成 · 3h」，不会变成普通 `$` 终端行。

### Phase B — Orca 式分区

Agents / Terminals 分组头 + agent 行按最近活动排序。

验收：一个 workspace 下「3 agents」可读，终端长任务不会混进 agent 列表。

### Phase C — 折叠态多头像（可选）

折叠父行最多 3 圆标 + `+N`。

## 10. 与现有规格的关系

- **保留**：呼吸环 / 黄环语义、绿点与 agent 互斥、选中在子节点、tab count=页签数、后台 workspace 仍显示运行态。  
- **修订**：`2026-08-26` 中「结束立刻消失头像」「不做状态列表」→ 改为「结束保留身份 + 文案状态」；列表仍是树节点，不是新面板。  
- **修订**：`2026-08-31` 中 Idle 槽「页签类型图标」对 agent 页签不适用 → agent 永远走品牌圆标。

## 11. 风险

- 右侧状态文案挤窄侧栏：必须做截断优先级；极窄时可只留圆标+环。  
- Done 残留让树变「历史坟场」：保留期 + 仅 InProgress/Blocked 点亮折叠绿点。  
- 标题改 agent 名后丢命令上下文：用 tooltip / 信息栏 / 中间面板补，不把命令塞回主标题。  
- 分组头与拖拽：第一期组内排序即可，跨 Agents↔Terminals 拖拽需产品再定。

## 12. 建议下一步

1. 产品确认 Phase A 文案语言（跟 UI locale）与 Done 保留时长。  
2. 按 Phase A 改 `ProjectTreeTabNode` + `render_tab_row`，补单测。  
3. 狗粮通过后再开 Phase B 分组头。

---

## 13. 修订（2026-10-15 晚，按产品反馈 + Orca 源码）

对照 `stablyai/orca`：`CompactAgentRow`、`AgentStateDot`、`getAgentRowPrimaryText`。

1. **废弃呼吸环绕头像**：状态与品牌图标分离。行顺序 `[AgentStateDot] [AgentIcon] [标题…] [model?] [相对时间]`。
2. **状态用图形，不用英文词**：Idle 灰点 · Working 黄转圈 · Done 绿勾 · Blocked/Failed 红点 · Waiting `?`。英文仅 tooltip / a11y。
3. **主标题**：`conversationName` → prompt / orchestration task 预览；空则 agent 类型名。不把 Idle/Done 拼进标题。
4. **取消 Agents / Terminals 分组**：靠品牌图标 vs `$`/绿点区分。

预览：`2026-10-15-workspace-agent-status-ui-preview.html`（v2）。

## 14. 品牌图标（2026-10-15）

落地时 **替换 Zap 现有 CLI logo**，对齐 Orca：

| Agent | Orca 来源 |
|-------|-----------|
| Grok | `src/shared/agent-icons/grok.png` |
| Claude | `ClaudeIcon` in `status-bar/icons.tsx`（fill `#D97757`） |
| Codex | `OpenAIIcon` in `status-bar/icons.tsx` |
| 其他有 favicon 的 CLI | `src/shared/agent-icons/*.png` / `AGENT_FAVICON_ASSETS` |

预览 v3：`2026-10-15-workspace-agent-status-ui-preview.html`

## 15. Model 芯片（2026-10-16）

行尾可选 model 芯片（Orca CompactAgentRow：`font-mono` / 10px / max≈72px），未知则不渲染。

| 来源 | 取数 |
|------|------|
| CLI agent | 启动命令 `--model` / `-m` / `--model=` → `CLIAgentSessionContext.model` |
| Oz 会话 | `Conversation::latest_model_chip_label()`（最近 `ModelUsed` 显示名 → model id → 请求的 `exchange.model_id`） |

归一化：`model_chip_label()` trim；`provider/name` 取末段；空串隐藏芯片。

Hover：`overlay_tool_tip_on_element` 显示完整 raw model（Orca `title={model}`）；芯片短名与 raw 相同时也显示（覆盖 72px 截断）。状态点同步挂英文 `accessibility_label` tooltip。

未接：插件事件中途上报的 model（CLI 目前只在检测启动命令时写入一次）。



## 16. Done→Idle 24h demotion（2026-10-16）

Done 残留超过 **24h**（`DONE_RESIDUAL_TTL`）后降级为 Idle：灰点 + 品牌标仍在，去掉绿勾。

| 机制 | 说明 |
|------|------|
| Compute-on-read | `presentation_phase` / `demote_done_phase` 按 `phase_changed_at` 年龄判定；`activities_from_terminal_sources` 写入树前 `apply_done_demotion` |
| 自刷新 | `DemotingDoneStatusGlyph` 在 layout 重算，并在 demotion 边界 `repaint_after`（与 RelativeTimeLabel 同路径，不靠后台 job） |
| 绿点 | `counts_as_busy` 走 presentation phase；Done / demoted Idle 均不计折叠绿点 |
| 无时间戳 | 缺少 `phase_changed_at` 时不降级，保持 Done 残留 |
| 相对时间 | 降级后仍用进入 Done 的时间戳，继续显示 `1d` / `2d`… |

边界单测：`done_just_under_24h_stays_done` / `done_at_24h_demotes_to_idle`。

## 17. 终端图标与右侧 meta 对齐（2026-10-16）

狗粮反馈：普通终端行 `$`/旧 iTerm「i」标不清晰；workspace 页签数与 agent 相对时间不在同一竖列。

| 项 | 做法 |
|----|------|
| 终端图标 | `app/assets/bundled/svg/iterm-prompt.svg` 改为深色圆角窗口 + 绿色 `>` + 光标块（仍走 `ITERM_PROMPT_ICON_PATH`；agent 品牌标不动） |
| 右侧 meta | `TREE_TRAILING_META_WIDTH`（32）右对齐页签数 / `now`·`Nm`；两侧共用 `tree_trailing_actions_width()` 两槽动作条（workspace：+ / ×；tab：关闭靠右），使 meta 右缘对齐 |

## 18. Waiting vs Blocked 分流（2026-10-16）

对齐 Orca：`permission` / `waiting` → 黄 `?`；真卡住 / 失败 → 红点。Zap 树相位：

| 来源事件 / 状态 | `WorkspaceAgentPhase` | 字形 |
|-----------------|----------------------|------|
| CLI `PermissionRequest` | **Waiting** | `?` 黄 |
| CLI `QuestionAsked` | **Waiting** | `?` 黄 |
| CLI `CLIAgentSessionStatus::Waiting` | Waiting | `?` |
| CLI `CLIAgentSessionStatus::Blocked`（真卡住，少见） | **Blocked** | 红点 |
| Oz `ConversationStatus::Blocked`（用户闸门：权限/确认） | **Waiting** | `?` |
| Oz `ConversationStatus::Error` | Failed | 红点 |

实现要点：

- `CLIAgentSessionStatus` 新增 `Waiting { message }`；权限/提问不再写入 `Blocked`。
- `needs_user_attention()` / `attention_message()` 覆盖 Waiting+Blocked（通知、rich-input 自动关、桌面 NeedsAttention）。
- `phase_from_cli_status` / `phase_from_conversation_status` 分流；a11y：Waiting=`Waiting for input`，Blocked=`Blocked`。
- 折叠绿点：Waiting 仍 `counts_as_busy`（与 InProgress/Blocked 同）。

未做：CLI 路径尚无产出「真 Blocked」的事件（红点留给未来/显式 stuck）；Failed 24h 降级另开。
