# 勾勾

本地优先的 Tauri 日记应用。业务数据由 Rust 写入应用沙盒内的 SQLite，不依赖账号或网络服务。

## 开发运行

安装依赖后，在项目根目录运行完整桌面应用：

```bash
npm install
npm run dev
```

这会同时启动 Vite 和标题为“勾勾”的 Tauri 窗口。日记、打勾、图片和备份功能只能在该独立窗口中使用。

仅调试 React 布局时可以运行：

```bash
npm run web:dev
```

此时浏览器页面不会连接 Rust 或 SQLite，只会显示运行方式说明。

## 验证

```bash
npm run build

cd src-tauri
cargo test --quiet
cargo fmt --check

cd ..
npm run tauri build -- --debug --no-bundle
```

开发模式的 Tauri 窗口会显示“备份验收”入口；发布前端不会显示该入口。
