# Briefy-pet Ver2-4 迭代说明（策略与前端统一版）

## 0. 文档信息

- 文档版本：v2.4
- 文档日期：2026-04-13
- 适用平台：macOS
- 基线文档：`Ver2.md`、`Ver2-1.md`、`Ver2-v3-data-dependencies.md`、`AGENTS.md`
- 本次目标：
  1. 让调度/推送真正落到 V3 的 module + bucket。
  2. 补全评分机制的可控性与一致性。
  3. 校验并增强个性化记忆链路。
  4. 完成主窗口与设置页视觉升级，保持桌宠素材不改。

---

## 1. Systematic Debugging 结论

### 1.1 Phase 1 根因调查

本次先做了代码级排查，不先拍脑袋改样式：

1. 调度根因：`list_due_sources` 实际按 `source_kind` 固定间隔（72h/6h/3h），没有使用 OPML 的 `module/bucket`。
2. 推送根因：提醒候选在 `service.rs` 中按 `source_kind` 固定 Top3，未按子分类差异化。
3. 评分根因：LLM prompt 未使用 module/bucket 语义；`fit_level` 完全依赖模型返回，服务端未做阈值校准。
4. 个性化根因：记忆已存在（偏好 + 行为总结），但总结维度主要是 discipline/source_kind，缺少 module/bucket 聚焦。
5. 前端根因：
   - `pet`/`bubble` 窗口创建未设置透明，容易出现“背景不透明”。
   - 主窗口视觉偏旧，信息层级与阅读器气质不足。

### 1.2 Phase 2 模式对齐

1. 数据结构对齐 V3：`rss_catalog_v3_unified.opml` 已含 `module/bucket`。
2. 运行时仍保留 `discipline/source_kind` 兼容层，不推翻现有调度器与数据库主链路。
3. 采用“V3 主结构 + 兼容字段并存”策略，降低改造风险。

### 1.3 Phase 3 假设验证

核心假设：如果将 `module/bucket` 打通到 runtime，并把频率/推送/阈值策略建立在该维度上，就能满足 Ver2-4 的“按学科+子分类”调度目标，同时不破坏现有功能。

### 1.4 Phase 4 实施结果

已完成落地，并配套文档与可运行验证。

---

## 2. V3 结构标准化（运行时）

### 2.1 模块规范化

运行时统一模块编码：

1. `technology`
2. `social_science`
3. `business`
4. `growth`
5. `news_opinion`
6. `entertainment`
7. `science`
8. `medicine`

兼容别名：

1. `personal_growth` -> `growth`
2. `news_and_social_opinion` -> `news_opinion`
3. `social-science` -> `social_science`

### 2.2 子分类规范化

兼容别名：

1. `social_science` 下 `research` -> `academic_frontier`

---

## 3. 抓取时间完整设定（问题 1）

说明：以下为 Ver2-4 运行时调度频率（按 module + bucket），未知项回退到 `source_kind` 默认频率。

| Module | Bucket | 频率 |
|---|---|---|
| technology | research | 24h |
| technology | official | 4h |
| technology | blogs | 6h |
| technology | community | 2h |
| technology | streaming | 3h |
| social_science | academic_frontier | 36h |
| social_science | blogs | 12h |
| social_science | community | 8h |
| business | blogs | 8h |
| business | community | 6h |
| business | streaming | 6h |
| growth | blogs | 12h |
| growth | community | 8h |
| growth | streaming | 8h |
| news_opinion | news | 1h |
| news_opinion | media_opinion | 2h |
| news_opinion | personal_opinion | 3h |
| news_opinion | streaming_opinion | 3h |
| news_opinion | community_opinion | 2h |
| entertainment | lite_pool | 6h |
| science | physics | 72h |
| science | chemistry | 72h |
| science | biology | 72h |
| medicine | academic_frontier | 48h |
| medicine | blogs | 12h |
| medicine | community | 8h |

默认回退（兼容）：

1. `academic-journal` -> 72h
2. `official-announcement` -> 6h
3. `technical-blog` -> 3h
4. `community-hotspot` -> 3h

---

## 4. 打分机制复核与修正（问题 2）

### 4.1 旧机制判断

旧机制“可用但不稳”：

1. 优点：已有批量评分、重试、单条降级、JSON 解析恢复。
2. 缺点：
   - prompt 缺少 module/bucket 语义。
   - `fit_level` 直接信任模型，容易波动。
   - 社区/观点类缺少降噪约束。

### 4.2 Ver2-4 修正

1. 输入增强：每篇文章增加 `MODULE`、`BUCKET`、`SCORING_HINT`。
2. 评分约束增强：明确要求 `fit_score` 以“相关性 + 可靠性 + 时效性”为核心。
3. 服务端校准：按 module/bucket 阈值重算 `fit_level`，不再直接信任模型 `fit_level`。
4. 兼容兜底：若模型 `fit_level` 缺失/异常，按分数自动推断。

### 4.3 分层阈值（摘要）

- 研究与学术类：高分门槛约 `76~79`。
- 官方类：高分门槛约 `74`。
- 博客类：高分门槛约 `72~74`。
- 新闻/观点/社区热点类：高分门槛约 `79~82`（更严格，抑制噪声）。

---

## 5. 每次推送篇数调整（问题 3）

说明：提醒候选改为按 `module+bucket` 分组，组内按分数排序后取 TopN，并设置全局上限。

### 5.1 分组 TopN

| Module | Bucket | 每批最多推送 |
|---|---|---|
| technology | research | 2 |
| technology | official | 2 |
| technology | blogs | 3 |
| technology | community | 2 |
| technology | streaming | 2 |
| social_science | academic_frontier | 2 |
| social_science | blogs | 2 |
| social_science | community | 1 |
| business | blogs | 2 |
| business | community | 1 |
| business | streaming | 1 |
| growth | blogs | 2 |
| growth | community | 1 |
| growth | streaming | 1 |
| news_opinion | news | 3 |
| news_opinion | media_opinion | 2 |
| news_opinion | personal_opinion | 1 |
| news_opinion | streaming_opinion | 1 |
| news_opinion | community_opinion | 1 |
| entertainment | lite_pool | 1 |
| science | physics | 1 |
| science | chemistry | 1 |
| science | biology | 1 |
| medicine | academic_frontier | 2 |
| medicine | blogs | 1 |
| medicine | community | 1 |

### 5.2 全局上限

- 单批提醒总条数上限：`12`。

这样避免“高活跃子池挤爆提醒气泡”。

---

## 6. 个性化机制完成度（问题 4）

### 6.1 已完成能力

1. 初始化偏好：每个已选 discipline 的偏好文本。
2. 行为采集：打开详情、收藏、提醒动作。
3. 每日记忆：可生成、可编辑、可关闭。
4. 评分输入：偏好 + 记忆摘要参与 LLM context。

### 6.2 Ver2-4 补强

1. 每日记忆新增 module/bucket 热点统计，摘要更贴近 V3 结构。
2. 仍保持 `memory_mode_enabled` 开关与人工编辑优先级。

结论：个性化链路已经可用且闭环，Ver2-4 达到“完整可运行”标准。

---

## 7. 前端设计系统（问题 5）

以下为“安静研究助手 + 轻陪伴感”的统一规范，已在主窗口与设置页落地。

### 7.1 风格定义说明

产品界面采用：

1. macOS 工具感为主（80%）。
2. 轻陪伴感为辅（20%）。
3. 强调阅读效率、低噪音、可长期停留。

视觉基调：暖中性纸感背景 + 淡分割线 + 单主强调色（蓝灰）+ 轻提醒色（杏棕）。

### 7.2 视觉关键词

1. 安静
2. 克制
3. 可信
4. 温和
5. 轻层次
6. 阅读器气质
7. 低打扰反馈

### 7.3 应避免风格

1. 赛博霓虹
2. 游戏化宠物 UI
3. 高饱和大片色块
4. 夸张炫光/强弹簧动效
5. 企业后台硬看板风

### 7.4 主窗口布局建议

1. 顶部粘性信息条：标题、状态、阅读/设置切换。
2. 双层概览卡：提醒状态、源池规模、最近抓取、待调度规模。
3. 主体双栏：左侧分组列表（新内容/全部/收藏），右侧详情。
4. 详情区分块：摘要、来源元信息、链接、时间、推荐理由。

### 7.5 气泡提醒布局建议

1. 一张主卡，不做系统警报样式。
2. 核心文案优先显示“你有 X 条新内容”。
3. 三个动作按钮层级明确：立即查看 / 稍后 30 分钟 / 忽略本次。
4. 动效使用短促淡入，不做跳动。

### 7.6 设置页布局建议

1. 分四段卡片：启动门槛、结构化兴趣、每日记忆、源池开关。
2. 学科偏好卡片网格化，输入区可直接编辑。
3. 源池按 `module -> bucket` 展示，不再只按旧 source_kind。
4. 保存动作固定在页尾，反馈明确。

### 7.7 组件级设计原则

1. 按钮：
   - 主按钮为蓝灰渐变，次按钮为浅底描边。
2. 卡片：
   - 轻圆角、细边、弱阴影，避免厚重浮层。
3. 标签：
   - 状态胶囊统一圆角与字号。
4. 列表项：
   - 信息分三层（标题/来源/评分元数据），选中态仅轻高亮。
5. 分区标题：
   - 使用 serif headline 提升阅读节奏，但不过度装饰。
6. 弹层/提醒：
   - 保持“便签+助手”的温和感，不做聊天气泡风。

### 7.8 动效原则

1. 状态可感知，存在感低。
2. 桌宠待机仅微弱呼吸（轻 bob）。
3. 气泡仅短淡入。
4. hover/active 有轻反馈，无持续夸张循环。

### 7.9 React + Tailwind + shadcn/ui 可执行规范

即使当前代码是 CSS 方案，也可按以下映射落到 Tailwind/shadcn：

1. 颜色 Token：
   - `bg-paper`/`bg-panel`/`line-soft`/`accent`/`hint`。
2. 圆角体系：
   - `rounded-xl`(12)、`rounded-2xl`(16)、`rounded-3xl`(18~24)、`rounded-full`。
3. 阴影体系：
   - `shadow-low`（阅读卡），`shadow-mid`（提醒卡）。
4. 组件映射：
   - `Card` 用于 `overview/settings/detail`。
   - `Badge` 用于状态与数量。
   - `Tabs` 用于阅读/设置切换。
   - `Switch`/`Checkbox` 用于开关。
5. 动效：
   - `transition-all duration-150 ease-out` 为默认。
   - `active:translate-y-0`, `hover:-translate-y-px`。

---

## 8. 本次代码落点

### 8.1 后端

1. 新增：`src-tauri/src/policy.rs`
   - module/bucket 规范化。
   - 抓取频率策略。
   - 推送配额策略。
   - 分数阈值与 fit_level 校准。
2. 更新：`src-tauri/src/models.rs`
   - `RssSource` / `FeedArticle` 新增 `module`、`bucket`。
3. 更新：`src-tauri/src/db.rs`
   - `source_catalog` 增加 `module`/`bucket` 持久化。
   - OPML 解析贯通 module/bucket。
   - `list_due_sources` 按新策略计算 due。
   - reminder 分区统计改为 module/bucket 维度。
   - daily memory 增加 module/bucket 热点摘要。
4. 更新：`src-tauri/src/service.rs`
   - 提醒选择按 module/bucket 配额。
   - 单批总上限 12。
   - fit_level 按策略阈值校准。
5. 更新：`src-tauri/src/llm.rs`
   - prompt 引入 module/bucket/scoring hint。
   - fit_level 缺失时按分数兜底。
6. 更新：`src-tauri/src/main.rs`
   - `pet`/`bubble` 窗口启用透明。

### 8.2 前端

1. 更新：`src/types.ts`
   - 新增 `SourceModule`、`SourceBucket`。
   - `RssSource` 增加 `module`、`bucket`。
2. 更新：`src/App.tsx`
   - 源池分组改为 module/bucket。
   - 文案与标签对齐 V3。
3. 更新：`src/styles.css`
   - 全面重做主窗口与设置页视觉体系。
   - 保持桌宠素材不变。
4. 更新：`src/main.tsx`
   - 仅在 pet 窗口禁用原生选择/拖拽，主窗口恢复正常输入与可选中行为。

---

## 9. 验收与运行检查项

### 9.1 结构验收

1. 运行时 source 已具备 `module + bucket + source_kind`。
2. 目录按 V3 模块/子分类可稳定显示。

### 9.2 功能验收

1. 抓取按 module/bucket 频率触发。
2. 提醒按 module/bucket 配额选取。
3. fit_level 经服务端阈值校准。
4. memory 摘要包含 module/bucket 行为特征。

### 9.3 体验验收

1. pet/bubble 窗口透明表现正常。
2. 主窗口阅读与设置页视觉统一，符合“安静研究助手”定位。

---

## 10. 后续建议（非阻塞）

1. 在快照里增加“当前策略命中”调试字段，方便运营校准频率与阈值。
2. 为策略表提供可配置化入口（但默认锁定 Ver2-4 推荐值）。
3. 增加针对策略函数的更完整单测（全 bucket 覆盖）。

---

## 11. 2026-04-13 前端重构补充（NetNewsWire 风格）

### 11.1 重构目标

在不改动桌宠素材和后端调度链路的前提下，将主阅读界面升级为「macOS 原生 RSS 阅读器」体验：

1. 三栏桌面布局（Sidebar / Timeline / Reader）
2. 侧栏与时间线支持拖拽调整宽度
3. 中栏与右栏独立滚动
4. Light / Dark 自适应
5. 保留既有 Tauri 数据联动（打开、收藏、气泡行为）

### 11.2 设计语言对齐说明

本次视觉实现采用 Claude_DESIGN 与 Notion_DESIGN 的共性原则，并约束到 macOS HIG 风格：

1. 字体：系统 UI 字体栈（SF Pro / Inter），阅读正文采用更友好的 serif 阅读栈。
2. 色彩：语义化中性灰 + system blue 作为唯一高强调色；深浅色自动切换。
3. 层次：极细分割线 + 低对比阴影，强调信息密度和阅读连续性。
4. 交互：桌面应用风格 hover、选中态和工具栏反馈，避免网页化夸张动效。

### 11.3 交互与结构变化

1. 左栏（约 250，可调）
   - Smart Feeds：Today / Unread / Starred
   - Folders：按 module 分组，支持折叠
   - 每个 feed 显示 favicon + 名称 + 未读徽标
2. 中栏（约 300~350，可调）
   - 卡片式文章时间线
   - 未读蓝点、标题两行截断、来源与时间、摘要预览
   - 当前选中文章采用系统蓝高亮
3. 右栏（流式）
   - 标题与元数据行
   - 工具栏：标记未读 / 星标 / 浏览器打开 / 分享
   - 阅读区最大宽度约 700，支持图片、引用块、链接样式

### 11.4 空状态与演示数据

为了保证首次运行可见完整布局，新增前端演示回退数据：

1. 当后端暂时无文章时，自动展示 demo feeds + demo articles。
2. demo 状态下仍可演示筛选、切换、星标、标记未读等交互。

### 11.5 本轮主要文件变更

1. `src/App.tsx`
   - 主窗口重构为 NetNewsWire 三栏布局
   - 新增可拖拽分栏、Smart Feeds、folder 折叠、工具栏动作
   - 新增空状态演示数据与阅读 HTML 渲染
2. `src/styles.css`
   - 全量更新为 macOS HIG 风格视觉系统
   - 增加 Light / Dark 语义变量
   - 三栏布局、独立滚动、分隔线与状态样式重写

### 11.6 完整测试记录

1. 前端构建：`npm run build` ✅
2. 后端测试：`cd src-tauri && cargo test` ✅（9 passed）
3. 打包验证：`npm run tauri:build:debug:sandbox-dmg` ✅
4. 产物：`src-tauri/target/debug/bundle/macos/Briefy-pet.app`、`src-tauri/target/debug/bundle/macos/Briefy-pet_0.1.0_aarch64.dmg`

---

## 12. 2026-04-13 推荐展示规则与阅读区修正

### 12.1 RSS 抓取 403 修正

本轮对抓取层补充了更完整的请求策略，用于降低站点拒绝率：

1. 为 RSS/Atom 请求统一设置 `User-Agent`、`Accept`、`Accept-Language`、`Cache-Control`、`Referer`。
2. 对 Reddit 源增加 403 回退：`www.reddit.com` / `reddit.com` 自动回退到 `old.reddit.com` 再试一次。
3. 告警降噪：`HTTP 403/404` 不再在主界面拼成长告警串，避免被站点策略拒绝的源长期刷屏；其余抓取错误仍保持告警。

### 12.2 主界面展示规则（重要）

为避免前端暴露后端沉淀池，本轮将主界面文章列表改为：

1. 仅展示“通过推荐规则”的帖子。
2. 具体判定为：文章必须已进入 `reminder_batch_articles`（即已通过 Ver2-4 的推荐链路：高契合候选 + module/bucket 分组配额 + 全局上限）。
3. `ranked_content_pool` 继续仅用于后端策略，不在前端全量展示。

### 12.3 右侧阅读区结构修正

移除统一示例图片，右栏固定为三段式：

1. `LLM摘要`
2. `LLM判断与用户兴趣打分原因`
3. `RSS抓到的原文`

为支持第 3 段，快照中的文章结构新增 `raw_content -> rawContent` 字段。

### 12.4 本轮代码落点

1. `src-tauri/src/rss.rs`
   - 请求头增强
   - Reddit 403 回退
2. `src-tauri/src/db.rs`
   - `list_articles` 改为仅返回进入提醒批次的文章
   - 返回 `raw_content`
3. `src-tauri/src/models.rs`
   - `ArticleRecord` 增加 `raw_content`
4. `src/types.ts`
   - `Article` 增加 `rawContent`
5. `src/App.tsx`
   - 右栏改为三段式文本结构
   - 去掉图片模板
6. `src/styles.css`
   - 右栏三段式样式

### 12.5 验证项

1. 重新执行 `npm run build`。
2. 重新执行 `cargo test`（确保后端结构变更后测试可通过）。

