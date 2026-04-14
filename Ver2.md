# Briefy-pet Ver2（mac 分支统一文档）

## 0. 文档说明

- 文档版本：v2.6（合并版）
- 更新时间：2026-04-14
- 适用分支：mac
- 文档性质：Ver2 全量归档 + 当前实现口径 + 验收基准

本文件已合并并取代以下历史拆分文档：

1. `Ver2_original.md`
2. `Ver2-1.md`
3. `Ver2-4.md`
4. `Ver2-5.md`
5. `Ver2-check.md`
6. `Ver2-v3-data-dependencies.md`

冲突优先级：

1. 产品边界与状态机：`AGENTS.md`
2. Ver2 结构与实施口径：本文件
3. README（运行与打包）：`README.md`

---

## 1. Ver2 演进总览

| 版本 | 目标 | 核心结果 |
|---|---|---|
| v2.0 | 建立 V3 信源结构 | 固定 8 个 module + 对应 bucket，明确 V3 为主目录 |
| v2.1 | 完成集中对齐落地 | 取消 science/medicine 暂缓，形成可执行映射口径 |
| v2.4 | 统一策略与前端视觉 | module/bucket 策略化调度与提醒，前端三栏阅读体验升级 |
| v2.5 | 稳定抓取链路与门禁 | 强制配置、首爬 7 天、重启增量、并发打分与池化推送收敛 |
| v2.6 | 完成左栏语义与去重修正 | Unread/Today/Favorites/History 语义重构，去重、时间线、CHECK 信息补全 |

---

## 2. V3 结构基线（固定口径）

### 2.1 Module

1. `technology`
2. `social_science`
3. `business`
4. `growth`
5. `news_opinion`
6. `entertainment`
7. `science`
8. `medicine`

### 2.2 Bucket

1. `technology`: `research` / `official` / `blogs` / `community` / `streaming`
2. `social_science`: `academic_frontier` / `blogs` / `community`
3. `business`: `blogs` / `community` / `streaming`
4. `growth`: `blogs` / `community` / `streaming`
5. `news_opinion`: `news` / `personal_opinion` / `streaming_opinion` / `community_opinion` / `media_opinion`
6. `entertainment`: `lite_pool`
7. `science`: `physics` / `chemistry` / `biology`
8. `medicine`: `academic_frontier` / `blogs` / `community`

### 2.3 资源文件基线

1. 主目录：`src-tauri/resources/rss_catalog_v3_unified.opml`
2. 历史输入：`src-tauri/resources/rss_catalog_v2_1_unified.opml`
3. science/medicine 输入：`src-tauri/resources/verified_science_medicine_rss.opml`

---

## 3. 数据依赖与运行时映射

### 3.1 目录层标准字段

1. `source_id`
2. `name`
3. `rss_url`
4. `module`
5. `bucket`
6. `source_kind`
7. `resource_type`
8. `language`
9. `enabled_by_default`
10. `origin_files`

### 3.2 `bucket -> source_kind` 映射（兼容当前调度器）

1. 学术类：`research` / `academic_frontier` / `physics` / `chemistry` / `biology` -> `academic-journal`
2. 官方类：`official` -> `official-announcement`
3. 博客类：`blogs` -> `technical-blog`
4. 社区与观点类：`community` / `streaming` / `*_opinion` / `lite_pool` -> `community-hotspot`

### 3.3 关键代码锚点

1. 模型与快照：`src-tauri/src/models.rs`
2. 目录/状态/池化：`src-tauri/src/db.rs`
3. 调度与推送：`src-tauri/src/service.rs`
4. 抓取策略：`src-tauri/src/rss.rs`
5. 前端主窗口：`src/App.tsx`

---

## 4. 抓取、打分、推送（当前实现口径）

### 4.1 抓取

1. 调度入口：`service::ensure_scheduler` 周期触发。
2. 抓取模型：并发抓取 + 失败重试 + 可重试错误二次回补。
3. 首次配置后：自动触发首爬，按每源最近 7 天纳入存量。
4. 重启后：触发强制增量，不等待常规间隔门槛。

### 4.2 打分

1. 对未打分文章批量并发评分（并发上限 20）。
2. prompt 引入 module/bucket 上下文。
3. 服务端按阈值校准 `fit_level`，避免模型直接漂移。

### 4.3 推送与沉淀池

1. 推送按 module/bucket 分组配额选取。
2. 单批提醒总上限 12。
3. 沉淀池按 module/bucket 各保留 Top 1000，超限淘汰低分尾部。

---

## 5. 前端实现（v2.6 完成项）

本节覆盖 2026-04-14 最新前端结构修正与信息补全。

### 5.1 左栏结构与语义

左栏固定顺序：

1. `Unread`
2. `Today`
3. `Favorites`
4. `History`

语义定义：

1. `Unread`：当前推送批次中尚未归档内容。
2. `Today`：Unread 归档后、属于当天推送的聚合内容。
3. `Favorites`：收藏或有笔记（评论）的内容。
4. `History`：除 Today 外的历史推送沉淀内容。

### 5.2 交互规则

1. 在 `Unread` 内点击同一条：
   - 第一次：选中提示。
   - 第二次：归档到 `Today`。
2. 可手动“标记未读”回流 `Unread`。

### 5.3 分类与时间线

1. `Today` / `Favorites` / `History` 统一按 `Module/Bucket` 树分类。
2. 中栏时间线按时间倒序。
3. 每条展示完整时间信息与缩写字段。

### 5.4 右栏 CHECK 信息

新增 CHECK 信息块：

1. `MOD`
2. `BKT`
3. `SRC`
4. `PUB`
5. `PUSH`
6. `FIT`
7. `FAV`
8. `NOTE`

### 5.5 去重保障

1. 文章层去重：`id + 指纹`。
2. 历史层去重：按批次时间优先并去重。
3. 目标：同帖不在同视图重复出现。

---

## 6. Ver2-check 要求对照（状态）

### 6.1 已落实

1. 保留 `mac` 分支进行持续交付。
2. 支持一套源码双平台打包脚本：
   - `scripts/build-macos-debug-sandbox-dmg.sh`
   - `scripts/build-macos-release-dmg.sh`
   - `scripts/build-windows-release-bundle.sh`
3. 抓取链路已收敛为“并发 + 重试 + 打分 + 池化推送”。

### 6.2 持续项

1. 多模型 provider 扩展（DeepSeek/GLM/Kimi/OpenAI/硅基流动）仍按后续迭代推进。
2. 用户自加 RSS 与完整重置交互继续按后续版本推进。

---

## 7. 验证基准

### 7.1 构建与测试

1. 前端构建：`npm run build`
2. Rust 测试：`cd src-tauri && cargo test`
3. mac 调试包：`npm run tauri:build:debug:sandbox-dmg`

### 7.2 交付产物

1. `.app`：`src-tauri/target/debug/bundle/macos/Briefy-pet.app`
2. `.dmg`：`src-tauri/target/debug/bundle/macos/Briefy-pet_0.1.0_aarch64.dmg`

---

## 8. 文档维护规则

1. 后续 Ver2 迭代不再新增 `Ver2-*.md` 分散文件。
2. 所有新增内容直接更新本文件，按版本新增小节。
3. 文档更新必须标注日期、目标、改动点、验证项。
