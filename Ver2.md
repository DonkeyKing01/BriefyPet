# Briefy-pet Ver2 PRD（规范版，V3信源结构）

## 0. 文档信息

- 文档版本：v2.0-rev-v3
- 文档日期：2026-04-12
- 文档性质：产品需求文档（PRD）
- 适用平台：macOS（交付形态沿用 Ver1-1）
- 关联文档：`AGENTS.md`、`Ver1-1.md`、`README.md`、`Ver2-1.md`

---

## 1. 文档定位

本文档定义 Briefy-pet Ver2 的正式信源架构与数据链路，核心是：

1. 以 V3 合并信源替换“分散目录认知”。
2. 建立固定的“模块 + 子池”结构，驱动抓取、排序、提醒。
3. 在不推翻现有技术栈（Tauri + React + Rust + SQLite）的前提下，完成可落地改造。

冲突优先级：

1. 交付形态/打包：以 `Ver1-1.md` 为准。
2. 产品基础规则/状态机：以 `AGENTS.md` 为准。
3. V3 信源结构/依赖关系：以本文档为准。

---

## 2. 背景与目标

当前版本已具备“抓取-评分-提醒-阅读”闭环，但信源组织仍存在问题：

1. 学科结构不稳定，运营与调度口径不一致。
2. science/medicine 在历史版本中存在“暂缓”逻辑，和当前规划不一致。
3. 文档结构、资源文件、运行时字段之间缺少明确映射。

Ver2 目标：

1. 统一为 V3 信源架构（8个模块）。
2. science/medicine 正式纳入有效体系。
3. 明确从 OPML 到运行时字段的依赖关系。

---

## 3. V3 信源架构（正式口径）

### 3.1 一级模块（Module）

1. 科技
2. 社科
3. 商业
4. 成长
5. 新闻观点
6. 娱乐
7. 科学
8. 医学

### 3.2 二级子池（Bucket）

1. 科技：研究 / 官方 / 博客 / 社区 / 流媒体
2. 社科：学术前沿 / 博客 / 社区
3. 商业：博客 / 社区 / 流媒体
4. 成长：博客 / 社区 / 流媒体
5. 新闻观点：新闻 / 个人观点 / 流媒体观点 / 社区观点 / 媒体观点
6. 娱乐：轻量高质量单层池
7. 科学：物理 / 化学 / 生物
8. 医学：学术前沿 / 博客 / 社区

### 3.3 关键约束

1. science 按学科分池（物理/化学/生物），不按博客/学术/社区拆分。
2. medicine 按学术前沿/博客/社区拆分。
3. 所有信源必须归属一个 module 与一个 bucket，不允许空值。

---

## 4. 范围定义

### 4.1 本期纳入

1. 合并 `rss_catalog_v2_1_unified.opml` 与 `verified_science_medicine_rss.opml`，产出 V3 主信源。
2. 根据 V3 架构调整 Ver2 / Ver2-1 文档口径。
3. 明确 PRD、资源、运行时模型的字段依赖关系。
4. 保持提醒交互语义不变（立即查看 / 稍后30分钟 / 忽略本次）。

### 4.2 本期不纳入

1. App Store 上架流程（签名、公证、沙盒）。
2. 云端账号与多端同步。
3. 用户自定义新增 RSS 源入口。

---

## 5. 详细需求（FRD）

### FR-01 信源整合与去重

1. 输入：
   - `src-tauri/resources/rss_catalog_v2_1_unified.opml`
   - `src-tauri/resources/verified_science_medicine_rss.opml`
2. 输出：
   - `src-tauri/resources/rss_catalog_v3_unified.opml`
3. 去重键：`normalized_url`（小写、去协议、去 fragment、去末尾斜杠、保留 query）。
4. 去重后保留来源追踪（origin）。

### FR-02 分类结构执行

1. V3 OPML 必须完整体现 8 个模块与对应子池。
2. 娱乐必须是单层池（不再使用“精选信源”二级组）。
3. science/medicine 必须作为有效模块纳入目录。

### FR-03 运行时兼容映射

在不重构现有 `SourceKind`（四类）前提下，V3 子池映射到运行时调度分区：

1. 学术相关：`research` / `academic_frontier` / `physics` / `chemistry` / `biology` -> `academic-journal`
2. 官方相关：`official` -> `official-announcement`
3. 博客相关：`blogs` -> `technical-blog`
4. 社区与流媒体相关：`community` / `streaming` / `*_opinion` / `lite_pool` -> `community-hotspot`

### FR-04 调度与分区

1. 调度频率保持：
   - `academic-journal`: 72h
   - `official-announcement`: 6h
   - `technical-blog`: 3h
   - `community-hotspot`: 3h
2. 每分区池上限 1000 条，超限淘汰低分尾部。
3. 默认每分区 Top3 汇总提醒。

### FR-05 个性化与记忆

1. 用户首次配置需完成 API Key + 至少1个模块偏好。
2. 每日记忆维持：生成、可编辑、可关闭。
3. 评分输入包含：模块偏好 + 子池上下文 + 最新确认记忆。

---

## 6. 数据模型与依赖关系

### 6.1 标准字段（目录层）

1. `source_id`
2. `name`
3. `rss_url`
4. `module`（V3一级模块）
5. `bucket`（V3二级子池）
6. `source_kind`（运行时调度兼容字段）
7. `resource_type`
8. `language`
9. `enabled_by_default`
10. `origin_files`

### 6.2 数据依赖链

1. `reference/*` + v2.1 OPML + science/medicine OPML
2. -> `rss_catalog_v3_unified.opml`（人工可读/运营主目录）
3. -> 运行时标准化目录（JSON，可由构建脚本生成）
4. -> `source_catalog` / `user_source_pool` / `source_fetch_state` / `ranked_content_pool`

### 6.3 与当前代码结构的对齐点

1. 调度维度仍依赖 `SourceKind` 四分区（`src-tauri/src/models.rs` + `src-tauri/src/db.rs`）。
2. V3 的 `module/bucket` 是运营结构，`source_kind` 是调度结构，二者并存。
3. `service.rs` 的 TopN 逻辑无需因 V3 结构重写。

---

## 7. 资源文件规范

### 7.1 本期主文件

1. `src-tauri/resources/rss_catalog_v3_unified.opml`（V3 主信源）
2. `src-tauri/resources/rss_catalog_v2_1_unified.opml`（历史基线，保留）
3. `src-tauri/resources/verified_science_medicine_rss.opml`（science/medicine 输入）

### 7.2 建议配套产物

1. `src-tauri/resources/rss-catalog-v3.json`（运行时目录）
2. `src-tauri/resources/rss-dedup-report-v3.md`（去重报告）

---

## 8. 验收标准

### A. 结构验收

1. V3 OPML 包含 8 个模块。
2. 各模块子池结构与本 PRD 完全一致。
3. science = 物理/化学/生物，medicine = 学术前沿/博客/社区。

### B. 数据验收

1. 总源数量、去重数量可追踪。
2. 每条源都有 module + bucket + source_kind 映射。
3. origin 可回溯到输入文件。

### C. 功能验收

1. 现有抓取、评分、提醒链路不回退。
2. 分区调度频率和 TopN 行为不回退。
3. memory 模式行为不回退。

---

## 9. 里程碑建议

### M1：结构落地

1. 产出 V3 OPML。
2. 完成 Ver2 / Ver2-1 文档结构统一。

### M2：运行时映射

1. 产出 V3 JSON（若启用）。
2. 校验 source_kind 分区映射与调度频率。

### M3：回归与发布

1. 验证抓取、评分、提醒、记忆全链路。
2. 验证 macOS 打包不回退。
