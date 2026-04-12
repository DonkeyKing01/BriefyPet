# Briefy-pet Ver2-1 PRD（集中开发版，V3结构对齐）

## 0. 文档信息

- 文档版本：v2.1-rev-v3
- 文档日期：2026-04-12
- 文档性质：产品需求文档（PRD）
- 适用平台：macOS（沿用 Ver1-1 交付目标）
- 关联文档：`AGENTS.md`、`Ver2.md`、`Ver1-1.md`、`README.md`

---

## 1. 文档定位

Ver2-1 是 Ver2 的集中落地版本。与旧版 Ver2-1 不同，本版本不再暂缓 science/medicine，改为全量纳入 V3 架构。

本版目标：

1. 一次性完成 V3 信源结构切换。
2. 完成 OPML 合并、分类口径统一和数据依赖对齐。
3. 保持当前提醒链路和桌宠交互不回退。

---

## 2. 与旧版 Ver2-1 的差异（强约束）

### 2.1 取消暂缓规则

1. science 不再暂缓，正式纳入有效信源池。
2. medicine 不再暂缓，正式纳入有效信源池。

### 2.2 采用 V3 统一结构

本版采用固定结构：

1. 科技：研究 / 官方 / 博客 / 社区 / 流媒体
2. 社科：学术前沿 / 博客 / 社区
3. 商业：博客 / 社区 / 流媒体
4. 成长：博客 / 社区 / 流媒体
5. 新闻观点：新闻 / 个人观点 / 流媒体观点 / 社区观点 / 媒体观点
6. 娱乐：轻量高质量单层池
7. 科学：物理 / 化学 / 生物
8. 医学：学术前沿 / 博客 / 社区

### 2.3 数据主目录

1. 以 `rss_catalog_v3_unified.opml` 作为主信源目录。
2. `rss_catalog_v2_1_unified.opml` 与 `verified_science_medicine_rss.opml` 作为输入基线保留。

---

## 3. 本版核心目标

1. 完成 V3 OPML 主目录生产与校验。
2. 完成 Ver2 / Ver2-1 文档口径统一。
3. 明确 module/bucket 到运行时字段（discipline/source_kind）的依赖映射。
4. 保持抓取、评分、提醒、记忆链路可持续演进。

---

## 4. 范围定义

### 4.1 本版纳入

1. 两份 OPML 合并与 URL 去重。
2. V3 分类结构重排（含 science/medicine）。
3. 依赖关系文档化（目录层 -> 运行时层）。
4. 回归提醒语义：立即查看 / 稍后30分钟 / 忽略本次。

### 4.2 本版不纳入

1. App Store 上架流程（签名、公证、沙盒）。
2. 云端账号体系与多端同步。
3. 用户自定义新增 RSS 源入口。

---

## 5. 数据输入与治理口径

### 5.1 输入文件

1. `src-tauri/resources/rss_catalog_v2_1_unified.opml`
2. `src-tauri/resources/verified_science_medicine_rss.opml`

### 5.2 输出文件

1. `src-tauri/resources/rss_catalog_v3_unified.opml`

### 5.3 去重规则

去重键 `normalized_url`：

1. 转小写。
2. 去协议差异（http/https）。
3. 去 fragment。
4. 去末尾斜杠（保留 query）。
5. 保留 query 参数。

---

## 6. 功能需求（Ver2-1执行口径）

### FR2-1-01 目录合并

1. 将 v2.1 主目录与 science/medicine 目录合并为 v3。
2. 保留 origin 追踪与重复统计。
3. 输出模块数必须为 8。

### FR2-1-02 结构一致性

1. science 必须仅包含：物理/化学/生物。
2. medicine 必须仅包含：学术前沿/博客/社区。
3. 娱乐必须为单层池。

### FR2-1-03 运行时兼容映射

在现有代码不大改前，采用以下映射：

1. `research`、`academic_frontier`、`physics`、`chemistry`、`biology` -> `academic-journal`
2. `official` -> `official-announcement`
3. `blogs` -> `technical-blog`
4. `community`、`streaming`、`*_opinion`、`lite_pool` -> `community-hotspot`

### FR2-1-04 调度与推送

1. 调度频率维持 72h / 6h / 3h。
2. 分区池维持每区 1000 上限。
3. 默认每区 Top3 合并提醒。

### FR2-1-05 个性化记忆

1. 每日兴趣总结可生成、可编辑、可关闭。
2. 初始化偏好 + 最新记忆参与评分输入。

---

## 7. 技术落点

### 7.1 后端

1. `src-tauri/src/models.rs`：维护运行时 discipline/source_kind 枚举与显示名。
2. `src-tauri/src/db.rs`：承接 source catalog、调度状态、分区池、记忆表。
3. `src-tauri/src/service.rs`：保持分区 TopN 汇总逻辑。

### 7.2 前端

1. `src/App.tsx`：配置页、源分组展示、记忆开关。
2. `src/types.ts`：与后端快照结构保持一致。

### 7.3 资源

1. `src-tauri/resources/rss_catalog_v3_unified.opml`（主目录）
2. `src-tauri/resources/rss_catalog_v2_1_unified.opml`（输入）
3. `src-tauri/resources/verified_science_medicine_rss.opml`（输入）

---

## 8. 验收标准

### A. 结构验收

1. V3 模块与子池结构与本 PRD 完全一致。
2. science/medicine 均为有效模块，不再标记“暂缓”。

### B. 数据验收

1. 合并后总量、去重量可追踪。
2. 每条记录可映射到运行时 source_kind。

### C. 功能验收

1. 调度频率与分区上限不回退。
2. 提醒动作语义不回退。
3. memory 行为不回退。

### D. 稳定性验收

1. 桌宠、气泡、主窗口核心体验不回退。
2. macOS 开发运行与打包流程不回退。

---

## 9. 里程碑（集中开发排布）

### M1：目录与文档

1. 输出 V3 OPML。
2. 完成 Ver2/Ver2-1 文档统一。

### M2：依赖对齐

1. 完成 module/bucket 到 source_kind 的映射说明。
2. 形成可执行的数据链路说明。

### M3：回归与发布准备

1. 全链路回归（抓取、评分、提醒、记忆）。
2. 打包验证（app/dmg）。
