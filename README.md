# BriefyPet

桌面上的信息雷达桌宠，用低打扰提醒帮用户筛出真正值得看的新内容。

## 1. 项目简介 

BriefyPet 不是传统 RSS 阅读器，而是一个常驻桌面的信息提醒伙伴。

它要解决的问题是：
- 信息源很多，但真正重要的内容很少
- 用户不希望一直手动刷订阅、刷网页、刷社媒
- 用户需要“被提醒”，但不希望被高频打扰

它适合：
- 需要持续跟踪科技、社科、医学、新闻等高价值信息的人
- 想把 RSS、LLM 摘要、兴趣筛选和桌面提醒整合到一个应用里的人
- 希望先被提醒、再决定是否深入阅读的人

典型使用场景：
- 桌宠平时驻留桌面
- 后端定时抓取 RSS 源
- LLM 根据兴趣偏好做摘要、打分和推荐理由
- 有高契合度内容时，用气泡提醒用户
- 用户点击后进入主窗口统一阅读、收藏、记录笔记

## 2. 核心功能

- 桌宠常驻桌面，支持托盘、置顶、主窗口联动
- 内置 RSS 源池，支持按学科与模块筛选启用
- LLM 生成摘要、契合度和推荐理由
- 只对高价值内容触发汇总提醒，降低打扰
- 主窗口支持 `Unread / Today / Favorites / History` 视图
- 本地 SQLite 存储文章、提醒批次、用户偏好与运行状态
- 支持 macOS 与 Windows 双平台构建

## 3. 快速开始 

安装依赖：

```bash
npm install
```

前端开发：

```bash
npm run dev
```

桌面应用开发模式：

```bash
npm run tauri -- dev
```

Windows 打包：

```bash
npm run tauri:build:windows:release
```

macOS 打包：

```bash
npm run tauri:build:mac:release-dmg
```

首次启动后的最小使用路径：

1. 打开应用
2. 在设置页填写可用的 LLM API Key
3. 勾选感兴趣学科并填写兴趣偏好
4. 保存设置，应用进入扫描
5. 有高契合度内容时通过桌宠和气泡提醒进入主窗口查看

## 4. 安装说明

环境要求：

- Node.js 18+
- npm 9+
- Rust stable
- Tauri CLI

平台说明：

- macOS：用于产出 `.app` / `.dmg`
- Windows：用于产出 `.msi` / `.exe`

Windows 打包依赖：

- WiX Toolset
- NSIS
- PowerShell 可执行环境

macOS 打包依赖：

- macOS 主机
- shell/bash 环境
- 对应架构的 Rust target

说明：

- 当前仓库是私有仓库
- `reference/` 作为本地参考目录，不进入版本控制
- 运行和打包都不依赖 `.env` 才能启动项目本体

## 5. 使用方法 

常用命令：

```bash
npm run dev
npm run build
npm run preview
npm run tauri -- dev
```

macOS 构建命令：

```bash
npm run tauri:build:debug:sandbox-dmg
npm run tauri:build:mac:release-dmg
```

Windows 构建命令：

```bash
npm run tauri:build:windows:debug
npm run tauri:build:windows:release
```

当前已验证的 Windows 产物目录：

```text
src-tauri/target/debug/bundle/msi
src-tauri/target/debug/bundle/nsis
src-tauri/target/release/bundle/msi
src-tauri/target/release/bundle/nsis
```

当前已保留的 macOS 产物目录：

```text
src-tauri/target/debug/bundle/macos
src-tauri/target/debug/bundle/dmg
src-tauri/target/release/bundle/macos
src-tauri/target/release/bundle/dmg
```

应用内配置方式：

1. 在设置页选择 LLM 提供商
2. 填写 API Key
3. 勾选学科并填写兴趣偏好
4. 调整 RSS 源启用状态
5. 按需开启自动启动与记忆模式

运行结果示例：

- RSS 抓取后，文章进入本地库
- 高契合度内容进入提醒批次
- 主窗口中可在 `Unread / Today / Favorites / History` 之间切换

## 6. 项目结构 

```text
src/                         React 前端界面
src-tauri/                   Tauri + Rust 后端
src-tauri/src/               窗口、抓取、数据库、调度、命令逻辑
src-tauri/resources/         打包资源与 RSS 主目录
public/                      桌宠素材与静态资源
scripts/                     macOS / Windows 打包脚本
config/                      预留配置目录
reference/                   本地参考项目目录（默认不进 Git）
Claude_DESIGN.md             设计参考文档
Notion_DESIGN.md             设计参考文档
```

关键代码定位：

- 桌宠 / 气泡 / 主窗口前端：`src/App.tsx`
- Tauri 启动与窗口创建：`src-tauri/src/main.rs`
- Tauri 命令入口：`src-tauri/src/commands.rs`
- 数据库与目录加载：`src-tauri/src/db.rs`
- 调度与抓取流程：`src-tauri/src/service.rs`
- RSS 抓取解析：`src-tauri/src/rss.rs`
- LLM 调用：`src-tauri/src/llm.rs`
- 策略与分桶：`src-tauri/src/policy.rs`

## 7. 配置说明 

当前项目主要通过应用内设置和本地数据库配置

应用内主要配置项：

- `apiKey`
- `llmProvider`
- `llmModel`
- `providerApiKeys`
- `autoStart`
- `disciplines`
- `memoryModeEnabled`
- `memorySummary`
- `rssSources`

主要数据与状态存储：

- 主数据库：应用数据目录下的 `briefy-pet.db`
- 推送/提醒数据库：应用数据目录下的 `briefy-pet-push.db`

Windows 常用本地排障位置：

```text
C:\Users\<User>\AppData\Roaming\com.briefypet.desktop\briefy-pet.db
C:\Users\<User>\AppData\Roaming\com.briefypet.desktop\briefy-pet-push.db
C:\Users\<User>\AppData\Roaming\com.briefypet.desktop\fetch-diagnostic.log
```

默认行为说明：

- 没有有效 API Key 时，应用停留在待配置状态
- 至少需要启用一个学科，且该学科偏好不能为空
- 抓取频率由 `module + bucket` 策略控制
- Windows `debug/release` 与 macOS `debug/release` 分别使用独立脚本

## 8. 开发日志

### Version1.0

第一版完成了桌宠产品的基础闭环：

- 桌宠、气泡、主窗口三层结构打通
- RSS 抓取、去重、本地 SQLite 存储接入
- API Key、兴趣偏好、RSS 源管理、自动启动等设置页能力落地
- LLM 摘要、契合度、推荐理由链路落地
- 高契合度内容进入提醒批次
- Windows & macOS 基础打包链路建立

### Version2.0

第二版在结构、策略和交付上做了收敛和增强：

- RSS 源治理升级，按 `module + bucket` 组织
- 学科和信源策略更明确，支持更稳定的抓取频率与提醒配额
- 主窗口演进为 `Unread / Today / Favorites / History` 阅读结构
- 引入本地推送池、历史流转和更清晰的文章归档逻辑
- 支持笔记、收藏、原文查看等阅读增强能力
- Windows & macOS 打包链路补齐
