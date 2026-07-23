# 勾勾

勾勾是一张私密、可回看的“人生日历”：用户自己决定哪一天值得留下一个勾；想写时，再用文字和照片留下当时。

当前项目是 Android 功能 Alpha，不是可公开发布的 Release Candidate。核心月历、勾、文字、图片、提醒和隐私锁已经可运行；好看的年历、加密便携备份、正式换机流程、可读导出、前端测试和 release 优化仍待完成。

当前 Alpha 的数据库、图片和导出备份尚未做应用层加密，Android Auto Backup 规则也还没有显式收紧。高敏感内容不要只保留这一份，使用前先确认有可恢复的外部备份。

## 文档

- [产品需求与工程规范](CODEX_MASTER_SPEC.md)：产品理念、v1 需求、技术方案、风险和开发顺序。
- [当前项目状态](PROJECT_STATUS.md)：现有能力、数据安全阻塞项和下一执行包。
- [Android Alpha 验收记录](docs/validation/ANDROID_ALPHA_2026-07.md)：截至 2026-07-21 的 vivo Android 15 真机证据。
- [Phase 5 实施归档](docs/archive/PHASE5_IMPLEMENTATION_2026-07.md)：旧实现上下文，仅供追溯。

## 技术栈

- Tauri 2
- React 18 + TypeScript
- Tailwind CSS 4
- Rust + rusqlite / SQLite
- Tiptap 3
- Kotlin / Swift 本地提醒插件

业务逻辑目前不依赖账号或应用自有云服务，Rust 是 SQLite 和私密资源的唯一数据所有者。当前数据库、图片和导出备份尚未完成应用层加密，因此请以 [当前项目状态](PROJECT_STATUS.md) 中的真实边界为准。

## 开发运行

安装依赖后，在项目根目录运行完整桌面调试应用：

```bash
npm install
npm run dev
```

这会同时启动 Vite 和标题为“勾勾”的 Tauri 窗口。仅调试 React 布局时可以运行：

```bash
npm run web:dev
```

浏览器模式不会连接 Rust 或 SQLite，只显示运行方式说明。

## 基础验证

```bash
npm run build

cd src-tauri
cargo test --quiet
cargo fmt --all -- --check

cd ..
git diff --check
```

Android 构建依赖本机 JDK、SDK、NDK 和 Rust targets，环境与最近一次设备验收信息记录在 [Android Alpha 验收记录](docs/validation/ANDROID_ALPHA_2026-07.md)。
