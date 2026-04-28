# BriefyPet

BriefyPet 是一个基于 Tauri + React + Rust + SQLite 的桌面信息雷达。它不是传统 RSS 阅读器，而是以桌宠常驻、提醒汇总、主窗口深读为核心的信息陪伴应用。

当前默认交付分支是 `mac`，目标产物为可直接安装的 macOS `.app` 与 `.dmg`。

## 产品定位

- 平时以桌宠形态常驻桌面，低存在感陪伴。
- 当高契合内容到达时，以汇总气泡进行低打扰提醒。
- 用户需要深入处理时，再进入主窗口统一查看、收藏、记录和配置。
- 所有核心数据本地落库，LLM 仅用于摘要、契合度判断、推荐理由和记忆提炼。

## 当前核心能力

### 桌宠与提醒

- 桌宠窗口支持透明、置顶、拖拽、托盘联动。
- 气泡窗口提供 `立即查看 / 稍后提醒 / 忽略本次`。
- 应用包含独立的帮助窗口与每周记忆回顾窗口。
- 托盘支持隐藏、显示、打开主界面与退出。

### 主窗口

- 当前主窗口左栏信息结构为 `Unread / Today / Favorites / History`。
- 中栏按时间倒序展示推送流。
- 右栏展示标题、摘要、来源、契合度、推荐理由、收藏状态与笔记。
- `Unread` 中首次点击只选中，二次点击会归档到 `Today`。

### 设置与偏好

- LLM 提供商、协议、Base URL、模型和 API Key 配置。
- RSS 源启用状态管理。
- 记忆模式开关与相关设置。
- 高级设置和数据重置入口。

### RSS 与本地数据

- 内置 RSS 目录来自 `src-tauri/resources/rss_catalog_0425.opml`。
- 当前信源语义已升级为 `一级学科 / 二级学科 / group` 三级结构。
- Rust 后端负责抓取、去重、评分、提醒批次、历史沉淀与本地 SQLite 存储。
- 仅高契合内容进入推送链路。

## 技术栈

- 前端：React 18 + TypeScript + Vite
- 桌面壳：Tauri 1.x
- 后端：Rust
- 本地数据库：SQLite
- 打包目标：macOS `.app` / `.dmg`，保留 Windows 打包脚本

## 目录结构

```text
src/                         React 前端
src-tauri/                   Tauri / Rust 后端
src-tauri/resources/         内置 RSS 目录等资源
src-tauri/icons/             应用打包图标
public/                      前端静态资源与桌宠素材
scripts/                     打包脚本
reference/                   本地参考项目与素材
Landing page/                独立落地页原型
AGENTS.md                    产品与交互规则主文档
Ver2.md                      合并后的版本说明
```

## 环境要求

- Node.js 18+
- npm 9+
- Rust stable
- macOS 本机环境
- 项目内已包含 `@tauri-apps/cli`

## 本地开发

安装依赖：

```bash
npm install
```

启动前端开发环境：

```bash
npm run dev
```

启动 Tauri 开发模式：

```bash
npm run tauri -- dev
```

前端构建：

```bash
npm run build
```

如需运行 Rust 侧测试：

```bash
cd src-tauri && cargo test
```

## 打包

### macOS debug

生成 debug `.app` 并通过 sandbox-safe 方式生成 `.dmg`：

```bash
bash scripts/build-macos-debug-sandbox-dmg.sh
```

### macOS release

生成 release `.app`，再通过 sandbox-safe 方式生成 `.dmg`：

```bash
bash scripts/build-macos-release-dmg.sh
```

### Windows release

保留 Windows 打包脚本：

```bash
bash scripts/build-windows-release-bundle.sh
```

## 常见产物路径

macOS release：

```text
src-tauri/target/release/bundle/macos/Briefy-pet.app
src-tauri/target/release/bundle/macos/Briefy-pet_0.1.0_aarch64.dmg
```

macOS debug：

```text
src-tauri/target/debug/bundle/macos/Briefy-pet.app
src-tauri/target/debug/bundle/macos/Briefy-pet_0.1.0_aarch64.dmg
```

## 文档约定

- 产品规则和交互状态以 `AGENTS.md` 为准。
- 版本演进说明统一收敛到 `Ver2.md`。
- `reference/` 主要用于本地参考，不作为正式运行依赖。
- 需要提交的改动应同时同步代码、README 和版本文档。

## 当前分支说明

- 当前分支：`mac`
- 当前远端：`origin/mac`
- 当前交付目标：优先保证 macOS 可安装包可用
