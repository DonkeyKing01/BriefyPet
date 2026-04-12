# Briefy-pet Ver2 V3 数据依赖关系说明

## 1. 目标

本文档用于把 V3 PRD 结构和当前代码实现之间的依赖关系明确化，解决“产品结构已更新，但运行时字段仍是旧模型”的落差问题。

## 2. 主结构（产品口径）

V3 固定为 8 个 module：

1. technology（科技）
2. social_science（社科）
3. business（商业）
4. growth（成长）
5. news_opinion（新闻观点）
6. entertainment（娱乐）
7. science（科学）
8. medicine（医学）

对应 bucket：

1. technology：research / official / blogs / community / streaming
2. social_science：academic_frontier / blogs / community
3. business：blogs / community / streaming
4. growth：blogs / community / streaming
5. news_opinion：news / personal_opinion / streaming_opinion / community_opinion / media_opinion
6. entertainment：lite_pool
7. science：physics / chemistry / biology
8. medicine：academic_frontier / blogs / community

## 3. 数据文件依赖

### 3.1 输入

1. src-tauri/resources/rss_catalog_v2_1_unified.opml
2. src-tauri/resources/verified_science_medicine_rss.opml

### 3.2 输出

1. src-tauri/resources/rss_catalog_v3_unified.opml（当前主目录）

### 3.3 可选中间产物（建议）

1. src-tauri/resources/rss-catalog-v3.json（运行时读取友好）
2. src-tauri/resources/rss-dedup-report-v3.md（去重与来源追踪）

## 4. 字段依赖映射

### 4.1 目录层标准字段（建议规范）

1. source_id
2. name
3. rss_url
4. module
5. bucket
6. source_kind
7. resource_type
8. language
9. enabled_by_default
10. origin_files

### 4.2 module -> 兼容 discipline（当前代码）

当前 Rust 端已有 discipline 枚举：technology / humanities / news / social-science / medicine / science / life / other。

建议兼容映射：

1. technology -> technology
2. social_science -> social-science
3. business -> other
4. growth -> life
5. news_opinion -> news
6. entertainment -> humanities
7. science -> science
8. medicine -> medicine

说明：

1. 这是一层运行时兼容映射，不改变产品层 8 模块定义。
2. 后续若扩展 discipline 枚举，可把 business/growth/entertainment 提升为独立枚举。

### 4.3 bucket -> source_kind（调度分区）

当前调度依赖四类 source_kind：academic-journal / official-announcement / technical-blog / community-hotspot。

建议映射：

1. research -> academic-journal
2. academic_frontier -> academic-journal
3. physics -> academic-journal
4. chemistry -> academic-journal
5. biology -> academic-journal
6. official -> official-announcement
7. blogs -> technical-blog
8. community -> community-hotspot
9. streaming -> community-hotspot
10. news -> community-hotspot
11. personal_opinion -> community-hotspot
12. streaming_opinion -> community-hotspot
13. community_opinion -> community-hotspot
14. media_opinion -> community-hotspot
15. lite_pool -> community-hotspot

## 5. 运行时依赖点

1. src-tauri/src/models.rs
   - 定义 discipline / source_kind 枚举及显示名称。
2. src-tauri/src/db.rs
   - source catalog、fetch state、ranked pools、daily memory 的读写。
3. src-tauri/src/service.rs
   - 按 source_kind 调度抓取和 TopN 汇总提醒。
4. src-tauri/src/rss.rs
   - 按 rss_url 拉取并去重。
5. src/App.tsx
   - 设置页与分组展示。

## 6. 兼容策略

1. 产品层以 module/bucket 为唯一结构来源。
2. 运行时调度继续使用 source_kind 四分区，避免大改调度器。
3. discipline 作为历史兼容字段保留，逐步过渡。
4. science/medicine 立即作为有效模块参与调度，不再标记 postponed。

## 7. 验证清单

1. V3 OPML 中是否包含 8 模块。
2. science 是否严格只有 physics/chemistry/biology。
3. medicine 是否严格只有 academic_frontier/blogs/community。
4. 每条源是否可计算出 source_kind。
5. 每条源是否可映射到兼容 discipline。
6. 提醒链路是否保持原语义：立即查看/稍后30分钟/忽略本次。
