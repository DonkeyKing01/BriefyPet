# BriefyPet

BriefyPet 是一个基于 Tauri + React + Rust 的 macOS 桌宠应用，目标是把高价值信息提醒做成低打扰的桌面陪伴体验。

当前仓库是私有协作仓库，`reference/` 目录只作为本地参考资料使用，不纳入版本控制。

## 目录说明

- `src/`
  前端界面代码，包含桌宠窗口、主窗口和提醒展示逻辑。
- `src-tauri/`
  Tauri + Rust 后端，包含窗口管理、RSS 抓取、LLM 分析、SQLite 存储等逻辑。
- `public/`
  前端静态资源，例如桌宠素材。
- `config/`
  本地配置预留目录。
- `reference/`
  本地参考项目目录，可自行放入参考仓库或素材，不会进入 Git。

## reference 约定

`reference/` 目录允许你自行放置参考项目，例如：

- `reference/WindowPet`
- `reference/AionUi`
- `reference/BriefFeed`
- `reference/clawd-on-desk`

这些内容只用于本地查看、拆解和对照实现，不作为当前项目源码的一部分提交。

如果后续增加新的参考项目，直接在 `reference/` 下新建目录即可，不需要额外改 Git 配置。

## 环境要求

- Node.js
- npm
- Rust
- Tauri CLI
- macOS 环境下构建安装包

## 安装依赖

```bash
npm install
```

## 本地开发

前端开发服务器：

```bash
npm run dev
```

Tauri 桌面应用开发模式：

```bash
npm run tauri -- dev
```

## 构建命令

前端构建：

```bash
npm run build
```

Rust 侧检查：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Tauri 桌面应用构建：

```bash
npm run tauri -- build
```

调试安装包构建：

```bash
npm run tauri -- build --debug
```

如需构建通用 mac 包，可先安装 Rust 双架构目标后执行：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri -- build --target universal-apple-darwin
```

## 当前打包产物

当前 macOS 安装包目标使用 Tauri 的 `app` 与 `dmg` bundle。

典型输出路径：

```text
src-tauri/target/release/bundle/dmg/Briefy-pet_0.1.0_aarch64.dmg
src-tauri/target/release/bundle/macos/Briefy-pet.app
```

其中 `.dmg` 是优先给测试用户分发的安装包；不要把 `target/debug`、裸二进制或未封装目录当成正式分发物。

## 版本控制说明

- 当前仓库只跟踪主项目源码
- `reference/` 整体忽略，不进入 Git
- 构建产物、数据库、日志、本地环境文件默认忽略

## 当前主要脚本

`package.json` 中可直接使用：

- `npm run dev`
- `npm run build`
- `npm run preview`
- `npm run tauri -- dev`
- `npm run tauri -- build`
- `npm run tauri -- build --debug`
