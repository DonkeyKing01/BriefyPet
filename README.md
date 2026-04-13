# Briefy-pet

Briefy-pet 是一个面向普通用户的“信息雷达桌宠”桌面应用。它不是传统 RSS 阅读器，而是一个常驻桌面的低打扰提醒伙伴：

- 平时以桌宠形态驻留桌面
- 有高价值新内容时，用汇总气泡提醒用户
- 用户需要进一步查看时，再进入主窗口统一管理内容

当前项目基于 `Tauri + React + Rust + SQLite` 实现，已经具备 Windows 可安装包产出能力，同时保留了 macOS `app/dmg` 打包配置。

## 1. 项目目标

第一版聚焦于“桌宠提醒 + RSS 抓取 + LLM 打分 + 本地阅读管理”这条完整闭环。

### 已纳入第一版

- 桌宠常驻桌面，支持拖拽、置顶、托盘管理
- 汇总气泡提醒
- 主窗口阅读、收藏、设置
- 内置 RSS 源抓取与开关管理
- 本地 SQLite 存储
- 用户填写自己的 LLM API Key
- 用户填写兴趣偏好文本
- LLM 负责摘要、契合度判断、推荐理由生成

### 第一版暂不实现

- 无 Key 模式
- 单独的新手引导页
- 用户自行新增 RSS 源
- 已读状态
- 复杂多档提醒策略
- 自定义“稍后提醒”时长

## 2. 核心模块

### 桌宠层 `pet-window`

- 透明、无边框、置顶、可拖拽
- 双击默认打开主窗口
- 若缺少有效配置，双击直接进入设置页

### 气泡层 `bubble-window`

- 汇总显示“你有 X 条新内容”
- 支持 `立即查看`、`稍后 30 分钟提醒`、`忽略本次`
- 不自动消失，直到用户处理

### 主窗口层 `main-window`

- 左侧内容列表，右侧详情阅读
- 内容分组为 `新内容`、`全部`、`收藏`
- 设置页内嵌在主窗口中

### RSS 抓取后端 `rss-fetcher`

- 抓取内置 RSS/Atom 源
- 去重与清洗
- 调用 LLM 生成摘要、契合度、推荐理由
- 仅 `高` 契合度进入提醒批次

### 数据层 `db`

- SQLite 本地存储
- 存储文章、提醒批次、抓取状态、设置、兴趣偏好、源开关等数据

## 3. 产品规则

### 状态机

- `loading`：应用启动与本地配置加载中
- `needs-config`：缺少有效 API Key 或必要配置
- `scanning`：正在抓取和分析内容
- `idle`：当前无待提醒内容
- `new-info`：存在高契合度新内容并已触发提醒

### 提醒规则

- 只推送契合度为 `高` 的内容
- 气泡是汇总提醒，不是逐条提醒
- `忽略本次` 只影响当前批次提醒，不删内容
- `稍后 30 分钟` 只延后当前批次，不影响后续新到内容

### 内容规则

- 第一版不做“已读”
- “新内容”表示“用户尚未点开详情”
- 点开详情后会移出 `新内容`，但保留在 `全部`
- 收藏只影响归类，不影响提醒逻辑

### API Key 规则

- 不支持无 Key 模式
- 没有有效 API Key 时，不进行抓取、摘要和打分

## 4. 当前架构与目录

```text
BriefyPet/
├─ src/                    # React 前端
├─ src-tauri/              # Tauri + Rust 后端
│  ├─ src/                 # 抓取、数据库、窗口、命令
│  ├─ resources/           # RSS 目录与打包资源
│  └─ tauri.conf.json      # Tauri 配置
├─ reference/              # 参考项目与历史参考素材
├─ scripts/                # 构建脚本
├─ AGENTS.md               # 早期产品/协作说明
├─ Ver*.md                 # 历次版本需求与迭代文档
└─ README.md               # 当前总说明
```

## 5. 版本演进总结

### 基线版本

项目最初围绕以下主链路设计：

1. 桌宠常驻桌面
2. RSS 抓取与内容去重
3. LLM 摘要与契合度判断
4. 高契合度内容进入气泡提醒
5. 主窗口中统一阅读、收藏和管理

### Ver1-1

重点是交付形态调整：

- 在不改业务逻辑的前提下，把交付目标从 Windows 安装包切到 macOS 本地安装包
- 明确保留 `Tauri + React + Rust` 架构
- 将 `app/dmg` 作为 macOS 打包目标

### Ver2 / Ver2-1

重点是 RSS 源治理与结构升级：

- 整理并去重 RSS 目录
- 引入 V3 统一信源结构
- 将信源按 `module + bucket` 组织，而不是仅靠历史散落目录
- 正式把 `science` 和 `medicine` 纳入主结构

V3 固定 8 个模块：

- `technology`
- `social_science`
- `business`
- `growth`
- `news_opinion`
- `entertainment`
- `science`
- `medicine`

### Ver2-v3-data-dependencies

重点是把“产品结构”和“运行时结构”对齐：

- 产品层面使用 `module + bucket`
- 运行时为兼容旧调度逻辑，仍保留 `source_kind`
- 建立 `module/bucket -> source_kind` 的映射关系

### Ver2-4

重点是策略统一与前端体验升级：

- 抓取频率正式按 `module + bucket` 管理
- 提醒配额按 `module + bucket` 分组控制
- 打分 prompt 与服务端阈值围绕 V3 结构校准
- 主窗口和设置页视觉统一升级
- RSS 抓取请求头更完整，降低部分源 `403`

## 6. 当前 RSS 与打分逻辑

### 首次配置后的首次抓取

- 不是按“最近几小时/几天”回溯
- 而是读取每个符合条件的 feed 当前可返回内容
- 每个源每次最多取前 `12` 条
- 是否抓取由源级别的 `last_fetched_at` 与抓取间隔决定

### 后续抓取

- 按源的 `last_fetched_at + 策略间隔` 判断是否到期
- 到期后重新拉取该 feed 当前前 `12` 条
- 再用文章身份字段做本地去重
- 不是按文章发布时间做严格的时间戳增量同步

### 当前抓取频率

当前策略以 `module + bucket` 为准：

- `technology / research`: 24h
- `technology / official`: 4h
- `technology / blogs`: 6h
- `technology / community`: 2h
- `technology / streaming`: 3h
- `social_science / academic_frontier`: 36h
- `social_science / blogs`: 12h
- `social_science / community`: 8h
- `business / blogs`: 8h
- `business / community`: 6h
- `business / streaming`: 6h
- `growth / blogs`: 12h
- `growth / community`: 8h
- `growth / streaming`: 8h
- `news_opinion / news`: 1h
- `news_opinion / media_opinion`: 2h
- `news_opinion / personal_opinion`: 3h
- `news_opinion / streaming_opinion`: 3h
- `news_opinion / community_opinion`: 2h
- `entertainment / lite_pool`: 6h
- `science / physics`: 72h
- `science / chemistry`: 72h
- `science / biology`: 72h
- `medicine / academic_frontier`: 48h
- `medicine / blogs`: 12h
- `medicine / community`: 8h

兼容兜底：

- `academic-journal`: 72h
- `official-announcement`: 6h
- `technical-blog`: 3h
- `community-hotspot`: 3h

### 当前提醒配额

- 每个 `module + bucket` 有各自 TopN 配额
- 单次提醒批次全局上限为 `12`
- `ranked_content_pool` 作为滚动排序池保留内容

## 7. 本轮实际改动

本轮不是只写文档，还对仓库做了几项关键修复。

### 1. 去掉对 `reference/*` 的构建依赖

原先 `src-tauri/build.rs` 会依赖一批已经不在仓库中的参考文件，导致：

- `cargo test` 无法跑通
- `tauri build` 无法继续

现在已改为只执行 `tauri_build::build()`，并删除不再需要的 build-time 依赖。运行时真正使用的 RSS 目录仍来自 `src-tauri/resources/`。

### 2. 保留 macOS 打包能力，不影响 Windows 出包

当前 `src-tauri/tauri.conf.json` 中仍保留：

- `targets: ["app", "dmg"]`

同时已经在 Windows 上重新验证并产出 NSIS 安装包。

### 3. 新增抓取诊断日志

后端现在会写入抓取诊断日志，帮助定位：

- 启动是否进入扫描
- 本轮到期源数量
- RSS 阶段抓到了多少文章
- 是否卡在 LLM 打分
- 最终是否入库与推送

日志路径：

- `C:\Users\Jinqy\AppData\Roaming\com.briefypet.desktop\fetch-diagnostic.log`

### 4. 修复“RSS 抓到但没有入库/推送”的核心问题

之前的问题是：

- RSS 成功后就提前更新 `last_fetched_at / last_success_at`
- 但文章要等整批 LLM 打分结束后才统一入库
- 一旦中间慢、失败或退出，就会出现“源被标记为已抓取，但文章一条没入库”

现在改成了更安全的两阶段流程：

1. RSS 抓到的新内容先进入 `pending_articles`
2. 后续按 LLM batch 分批处理
3. 每个 batch 返回后立刻：
   - 写入 `articles`
   - 更新 `ranked_content_pool`
   - 累积 reminder 候选
4. 只有该源内容真正处理完成后，才更新 `last_fetched_at / last_success_at`

这样即使处理中断，也不会整批吞掉结果。

### 5. 前端扫描提示更明确

扫描时会提示用户当前仍在处理中，避免误以为结果应该边扫边立刻新增。

### 6. 新增“重新初始化”为新用户功能

设置页新增危险操作按钮：

- `清空数据并重新初始化`

该操作会二次确认，并清理：

- API Key
- 兴趣勾选
- 抓取文章
- 待处理文章
- 内容池
- 提醒批次
- 抓取状态
- 本地行为与记忆数据

执行后应用会回到“首次使用、需要重新配置”的状态。

## 8. 资源文件现状

`src-tauri/resources/` 目前包含：

- `rss_catalog_v3_unified.opml`
- `rss_catalog_v2_1_unified.opml`
- `verified_science_medicine_rss.opml`
- `rss-catalog-v2-1.json`
- `rss-sources.json`
- `rss-dedup-report-v2-1.md`

当前运行时实际优先使用：

- `rss_catalog_v3_unified.opml`

回退时会使用：

- `rss-catalog-v2-1.json`

## 9. 开发与构建

### 环境

- Node.js
- npm
- Rust / Cargo
- Tauri CLI
- Windows 打包需要 NSIS 环境
- macOS 打包需要在 macOS 主机执行

### 常用命令

安装依赖：

```bash
npm install
```

前端开发：

```bash
npm run dev
```

Tauri 开发：

```bash
npm run tauri -- dev
```

前端构建：

```bash
npm run build
```

Rust 测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Windows 安装包：

```bash
npm run tauri -- build -- --bundles nsis
```

macOS 调试打包脚本：

```bash
npm run tauri:build:debug:sandbox-dmg
```

说明：

- `tauri.conf.json` 保留了 `app/dmg` 配置，macOS 打包能力仍在
- 但 `app/dmg` 需要在 macOS 主机上执行，不能在当前 Windows 主机直接产出

## 10. 本地排障入口

数据库路径：

- `C:\Users\Jinqy\AppData\Roaming\com.briefypet.desktop\briefy-pet.db`

抓取诊断日志：

- `C:\Users\Jinqy\AppData\Roaming\com.briefypet.desktop\fetch-diagnostic.log`

这两个文件是当前排查“是否抓到了”、“是否入库了”、“为什么没推送”的第一入口。

## 11. 当前状态

截至本 README 更新时：

- Windows 编译、测试、NSIS 打包可用
- macOS `app/dmg` 配置保留
- RSS -> pending -> LLM batch -> 入库 -> 提醒 的新链路已落地
- 设置页支持重新初始化为新用户

## 12. 最近更新

### 2026-04-13

- 收敛项目文档，使用当前 README 作为统一主文档，删除旧的 `AGENTS.md` 与 `Ver*.md`
- 去掉 `build.rs` 对 `reference/*` 的构建期强依赖，恢复 Windows 编译与打包链路
- 新增后端抓取诊断日志，日志写入 `C:\Users\Jinqy\AppData\Roaming\com.briefypet.desktop\fetch-diagnostic.log`
- 把抓取流程改为 `RSS -> pending_articles -> 分批 LLM 打分 -> articles/reminders`
- 将 `last_fetched_at / last_success_at` 的更新时间后移到源内容处理完成之后
- 新增设置页“清空数据并重新初始化”功能，支持回到新用户状态
- 修复 `articles.guid` 唯一约束导致的入库失败：
  - RSS 空 `guid` 会自动回退为稳定值
  - 入库前去重补充 `guid` 检查
  - 启动时会自动修复库中历史空 `guid`
- 重新验证：
  - `cargo test --manifest-path src-tauri/Cargo.toml` 通过
  - `npm run tauri -- build -- --bundles nsis` 通过
- 最新 Windows 安装包：
  `src-tauri/target/release/bundle/nsis/Briefy-pet_0.1.0_x64-setup.exe`
