# Skill 生成（本期含 create-skill AI 代写）

日期：2026-10-16  
分支：`feat/repository-workspaces`  
预览：[`2026-10-16-skill-generate-wizard-preview.html`](./2026-10-16-skill-generate-wizard-preview.html)  
相关能力：`resources/bundled/skills/create-skill/`；UI 调 skill 对标 `FixSettingsWithOz`（`workspace/view.rs`：`active_bundled_skill` + `SlashCommandRequest::InvokeSkill`）。

## 本期范围（MVP-with-AI）

**必须有**

- Skill 管理器入口引导用户收集意图，并**启动 bundled `create-skill` agent 会话**代写草稿（不是纯表单模板落盘）。
- 落点二选一：仓库 `.agents/skills/<name>/SKILL.md`，或个人 `~/.warp/skills/<name>/SKILL.md`。
- 命名撞名检测（库存按名合并 / pin），写入前再次确认覆盖。
- 用户审阅 agent 产出后确认写入（或确认 agent 已写入的路径），并打开 `SKILL.md`。

**本期不做 / 不强绑 UI**

- 不把 create-skill 的完整 eval / benchmark / description-improver 做成独立向导页（用户可在同一 agent 会话里继续按 skill 流程做）。
- 不自动 git commit；不在管理器里批量建 `scripts/` / `references/` 资源包（由 agent 在对话中按需创建）。
- 不接其它 CLI agent 的 skill 创建器；只走 Zap Oz + bundled `create-skill`。

## 入口

1. Skill 管理器顶栏主按钮「生成 Skill」。
2. 列表空态 / 「还没有合适的 Skill？」CTA。
3. （可选后续）斜杠 `/create-skill` 与现有 invoke 对齐；本期以管理器向导为主。

需 AI 可用；不可用时按钮禁用并提示开启 AI（同 tab-config footer 的 AI gate）。

## 产品流程

1. **收集意图**：一句话目标 + 可选类型提示（工作流 / 审查 / 文档 / 其它）；写入后续 prompt，不直接当最终正文。
2. **触发描述**：用户填写/改写 description 要点（做什么 + 何时用）；作为 create-skill 的触发输入。
3. **落点**：仓库 Agents vs 个人 Zap；展示将写入的绝对路径模式。
4. **命名 + 撞名**：slug 校验；若库存已有同名，告警标明当前生效来源（default / pin）与路径；可改名，或「仍用此名」并在写入时按 duplicate/覆盖规则再确认。
5. **启动 AI 草拟**：新开 agent 模式终端页签，`InvokeSkill { skill: create-skill, user_query: 预填上下文 }`；向导进入「AI 草拟中」，可跳转到该会话。
6. **审阅与确认写入**：agent 按 create-skill 访谈/起草；用户在会话里改，或在向导审阅摘要后确认打开已写文件；库存刷新。

## 撞名规则

- 检测源：`SkillManager` 库存同名项（含 Agents / Zap / Claude / 内置展示名）。
- 命名步：软阻塞（警告 + 建议改名）；允许继续。
- 写入步：若目标路径已存在文件 → 硬确认覆盖；若仅同名不同路径 → 写入后成为 duplicate，沿用现有 pin / 按名合并，不在本期改合并算法。
- 禁止静默覆盖用户未确认的已有 `SKILL.md`。

## Agent 如何调用

对标设置修复路径（`FixSettingsWithOz`）：

1. `SkillManager::active_bundled_skill("create-skill", ctx)` 取 `ParsedSkill`。
2. `add_terminal_pane_in_agent_mode` 打开新会话。
3. `ai_controller.send_slash_command_request(SlashCommandRequest::InvokeSkill { skill, user_query: Some(prefilled) })`。

预填 `user_query` 建议包含：目标简述、触发 description 草稿、建议 `name`、目标目录绝对路径、是否允许覆盖、语言偏好（中文说明可写进 skill）、以及「先写出可运行的单文件 SKILL.md；完整 eval 仅在用户明确要求时再跑」。

Bundled skill 走 `InvokeSkill` 加载（非 `read_skill`），与现有 prompt 渲染约定一致。

## 与「仅模板 MVP」的差异

| | 旧预览（模板） | 本期（AI） |
|---|---|---|
| 正文来源 | 本地模板拼接 | `create-skill` agent 起草 |
| 最后一步 | 「预览 → 写入」 | 「启动 AI → 草拟中 → 审阅 / 打开」 |
| Eval | 明确不做 | UI 不强制；会话内可继续 |

## 实现备忘（尚未写 Rust）

- UI：`app/src/skill_manager/panel.rs` 增加生成入口与向导 / 状态；打开 agent 的逻辑可放在 `workspace/view` 或 panel 事件上抛。
- 预填文案与路径解析需覆盖无仓库 cwd（个人落点仍可用；仓库落点需有 active repo）。
- Telemetry：生成入口点击、启动 create-skill、确认写入、撞名继续/改名（事件名实现时定）。
