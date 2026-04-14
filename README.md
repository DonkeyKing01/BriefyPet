# BriefyPet (mac branch)

BriefyPet 是一个基于 Tauri + React + Rust + SQLite 的桌宠信息雷达应用。

目标不是做纯阅读器，而是做低打扰的信息提醒伙伴：

1. 桌宠常驻桌面。
2. 有高价值内容时做汇总提醒。
3. 需要深读时进入主窗口统一处理。

---

## 1. 当前分支状态

当前分支：`mac`

当前主要交付目标：

1. 以 macOS 可安装应用（`.app` / `.dmg`）为主交付形态。
2. 保持一套源码，支持 mac 与 windows 两套打包脚本。

文档入口：

1. `AGENTS.md`：产品边界与状态规则（单一真相源）。
2. `Ver2.md`：Ver2 全量合并文档（架构、迭代、验收）。

---

## 2. 核心能力

1. 桌宠常驻、置顶、托盘管理。
2. RSS 抓取 + LLM 摘要/契合度/推荐理由。
3. 仅高契合内容触发提醒批次。
4. 主窗口阅读、收藏、笔记与设置统一管理。
5. 本地 SQLite 存储（文章、设置、提醒批次、记忆）。

---

## 3. 最新前端信息架构

主窗口左栏固定为：

1. `Unread`
2. `Today`
3. `Favorites`
4. `History`

语义说明：

1. `Unread`：当前推送批次未归档内容。
2. `Today`：Unread 归档后的当天推送聚合。
3. `Favorites`：收藏或有笔记内容。
4. `History`：除 Today 外的历史推送沉淀。

交互规则：

1. Unread 中同一条首次点击仅选中，第二次点击归档到 Today。
2. 分组统一按 `Module/Bucket` 展开。
3. 中栏时间线按倒序展示并显示时间。
4. 右栏支持 CHECK 信息块（MOD/BKT/SRC/PUB/PUSH/FIT/FAV/NOTE）。

---

## 4. 目录结构

```text
src/                      # React 前端
src-tauri/                # Rust 后端 + Tauri 壳层
public/                   # 静态资源（含桌宠素材）
scripts/                  # 打包脚本（mac/windows）
reference/                # 本地参考资料（默认忽略，不进入 Git）
```

---

## 5. 环境要求

1. Node.js 18+
2. npm 9+
3. Rust stable
4. Tauri CLI（项目 devDependencies 已含 `@tauri-apps/cli`）
5. macOS（进行 mac 打包时）

---

## 6. 快速开始

安装依赖：

```bash
npm install
```

前端构建：

```bash
npm run build
```

后端测试：

```bash
cd src-tauri && cargo test
```

Tauri 开发运行：

```bash
npm run tauri -- dev
```

---

## 7. 打包命令

### 7.1 macOS

debug app + sandbox-safe dmg：

```bash
npm run tauri:build:debug:sandbox-dmg
```

release dmg：

```bash
npm run tauri:build:mac:release-dmg
```

### 7.2 Windows

release bundle（msi + nsis）：

```bash
npm run tauri:build:windows:release
```

---

## 8. 常见产物路径

debug：

```text
src-tauri/target/debug/bundle/macos/Briefy-pet.app
src-tauri/target/debug/bundle/macos/Briefy-pet_0.1.0_aarch64.dmg
```

release：

```text
src-tauri/target/release/bundle/macos/Briefy-pet.app
src-tauri/target/release/bundle/dmg/Briefy-pet_0.1.0_aarch64.dmg
```

---

## 9. 开发约定

1. `reference/` 仅作本地参考，不进入 Git。
2. `tmp_*` 调试产物不入库。
3. 文档更新统一写入 `Ver2.md`，不再拆分 `Ver2-*.md`。
4. 重大功能变更需同时更新：代码 + `Ver2.md` + `README.md`。

---

## 10. 质量基线

每次准备提交前，建议至少执行：

```bash
npm run build
cd src-tauri && cargo test
```

如涉及安装包验证，再执行：

```bash
npm run tauri:build:debug:sandbox-dmg
```
