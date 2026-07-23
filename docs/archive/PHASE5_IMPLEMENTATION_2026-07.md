# Phase 5 实施归档

> 历史快照：本文保留截至 2026-07-13 的实施上下文，其中若与当前代码或产品规范冲突，以 [`CODEX_MASTER_SPEC.md`](../../CODEX_MASTER_SPEC.md) 和 [`PROJECT_STATUS.md`](../../PROJECT_STATUS.md) 为准。
>
> 不要按本文的“必须继续检查”或旧阶段顺序继续执行。

更新时间：2026-07-13（Asia/Shanghai）

## 当前目标

先完成 Phase 5 设计并更新 `CODEX_MASTER_SPEC.md`，再实现以下最小闭环：

- 本地晚间提醒；
- 系统生物识别隐私锁；
- 系统/浅色/深色主题；
- 减少动画、触感反馈和基础无障碍；
- Android 首发，同时保留并实现 iOS 对等插件接口。

## 已完成设计

`CODEX_MASTER_SPEC.md` 已从 v1.3 更新为 v1.4，并新增完整的 Phase 5 章节，包括：

- 范围和禁止项；
- `user_settings` 设置模型及默认值；
- Rust IPC 契约；
- `gougou-reminder` 移动插件契约；
- Android AlarmManager、通知权限、精确闹钟降级和重启恢复规则；
- iOS 未来 30 天通知队列规则；
- 生物识别启用、关闭、冷启动和后台 30 秒重锁状态机；
- 主题、减少动画、触感、动态字体和无障碍要求；
- 真机验收矩阵和 Phase 5 结束条件。

规范中的关键设计决定：

1. `user_settings` 是业务设置唯一事实来源。
2. SharedPreferences/UserDefaults 只保存可由设置重建的调度缓存。
3. 原生插件不得读写 SQLite、Markdown、图片或备份。
4. Android 默认使用不精确提醒；精确授权不可用时自动降级。
5. 隐私锁是应用访问门槛，不宣称数据库或备份已加密。
6. 不支持的平台必须明确返回 `unsupported`，不得显示虚假成功。

## 已完成实现

### 1. Rust 设置后端

位置：`src-tauri/src/lib.rs`

已新增：

- `AppSettings`；
- `ReminderSettings`；
- `PrivacySettings`；
- `AppearanceSettings`；
- `AccessibilitySettings`；
- `get_app_settings`；
- `update_app_settings`；
- 设置默认值、读取、事务写入和严格校验；
- 提醒时间、主题、暂停日期、静默日校验；
- 备份设置键白名单，未知键不允许通过导入；
- `get_reminder_status`；
- `request_reminder_permission`；
- `sync_reminder`；
- `get_biometric_platform_status`；
- 从未来已打勾日期生成原生提醒跳过列表；
- 移动端启动时按数据库设置重新同步提醒；
- 打勾成功后由前端重新同步提醒。

当前 Rust 测试已从 13 个增加到 15 个，新增覆盖：

- 设置默认值；
- 设置完整往返；
- 非法时间；
- 重复静默日；
- 非法主题；
- 备份未知设置键。

### 2. 本地提醒插件骨架

位置：`plugins/gougou-reminder/`

插件由 Tauri CLI 生成，当前 Rust 侧命令和模型包括：

- `getStatus`；
- `requestPermission`；
- `syncSchedule`；
- `cancelAll`；
- `takeNotificationTarget`（正在完成接线）；
- `ReminderSchedule`；
- `ReminderStatus`；
- `NotificationTarget`。

桌面实现明确返回 `supported=false`，保证桌面开发构建不会伪造原生能力。

### 3. Android 提醒实现

位置：`plugins/gougou-reminder/android/`

已新增：

- `GougouReminderPlugin.kt`；
- `ReminderScheduler.kt`；
- `ReminderReceiver.kt`；
- `ReminderBootReceiver.kt`。

已实现逻辑：

- Android 13+ `POST_NOTIFICATIONS` 请求；
- AlarmManager 单次安排下一个合格日期；
- 默认不精确；
- 用户首次开启“尽量准时”时跳转精确闹钟系统授权；
- 未获授权时自动使用不精确 Alarm；
- 静默星期；
- 暂停截止日期；
- 已打勾日期跳过；
- 通知渠道和本地通知；
- 通知点击携带 `gougouTargetDate`；
- `BOOT_COMPLETED` 与 `MY_PACKAGE_REPLACED` 后恢复调度；
- 权限撤销后不继续伪装为已安排。

AndroidManifest 已声明：

- `POST_NOTIFICATIONS`；
- `RECEIVE_BOOT_COMPLETED`；
- `SCHEDULE_EXACT_ALARM`；
- 两个受控 Receiver。

### 4. iOS 提醒实现

位置：`plugins/gougou-reminder/ios/Sources/GougouReminderPlugin.swift`

已实现逻辑：

- 查询和请求本地通知授权；
- 取消旧 `gougou-reminder-*` 队列；
- 排除静默日、暂停日期和已打勾日期；
- 预排未来 30 条单次通知；
- 通知 `userInfo` 携带 `gougouTargetDate`；
- 使用 DispatchGroup 等待添加请求后再返回状态；
- UNUserNotificationCenterDelegate 接收通知点击并暂存目标日期；
- `takeNotificationTarget` 单次消费目标日期。

### 5. 生物识别隐私锁

已安装：

- npm：`@tauri-apps/plugin-biometric@2.3.2`；
- Rust：`tauri-plugin-biometric = "2.3.2"`。

已实现：

- 移动端条件注册官方 biometric 插件；
- 移动端 capability：`src-tauri/capabilities/mobile.json`；
- 开启和关闭隐私锁前调用系统验证；
- `allowDeviceCredential: true`；
- 冷启动读取设置后先显示不含日记内容的锁屏；
- 验证失败保持锁屏；
- 后台超过 30 秒重新锁定；
- 桌面报告不支持，避免导入移动设置后把桌面永久锁死。

### 6. 设置页、主题与辅助功能

新增文件：

- `src/settings.ts`；
- `src/SettingsView.tsx`。

已实现设置页区域：

- 提醒开关；
- 时间选择；
- 尽量准时；
- 静默日；
- 暂停一周/恢复；
- 隐私锁；
- 系统/浅色/深色主题；
- 减少动画；
- 触感反馈；
- 本地数据说明。

其他前端实现：

- 月历页新增正式“设置”入口；
- 设置页 Android 系统返回回到月历；
- 提醒遵循“权限 → 原生同步 → Rust 持久化”的启用顺序；
- 同步或数据库保存失败时尝试恢复此前原生提醒计划；
- 打勾成功后按设置调用轻触感；
- `data-theme` 和 `data-reduce-motion` 即时应用；
- 深色主题基础颜色覆盖；
- 系统 `prefers-reduced-motion` 与用户减少动画设置均生效。

## 已完成验证

最近一次已通过：

```text
npm run build
cargo test --quiet --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --debug --no-bundle
git diff --check
```

结果：

- TypeScript 与 Vite 生产构建通过；
- Rust 15 个测试全部通过；
- 桌面 Tauri debug 无打包构建通过；
- `tauri-plugin-biometric` 和本地 `tauri-plugin-gougou-reminder` 的桌面 Rust 侧编译通过；
- 当前前端主包约 651 KB，仍有既有的 Rollup 大包告警。

## 通知点击应用层接线（已完成）

1. `NotificationTarget` 已加入本地插件 Rust 模型。
2. 移动/桌面插件方法 `take_notification_target` 已加入。
3. 插件 Rust command 和 build command 名称已加入。
4. Android `takeNotificationTarget` 已加入，读取并清除 Activity Intent 中的 `gougouTargetDate`。
5. iOS `takeNotificationTarget` 和通知 delegate 已加入。
6. 应用 Rust 主命令已暴露并注册 `take_reminder_target`，且会丢弃非法民用日期。
7. React 启动流程会单次消费目标日期；读取失败时降级为普通启动，不阻断本地日记。
8. `App` 使用合法目标初始化选中日期及月份，只定位月历，不自动打开编辑器。
9. 启动初始化增加单次保护，避免 React `StrictMode` 重复消费一次性目标。

本轮已重新通过：

```text
cargo fmt --manifest-path src-tauri/Cargo.toml --all
git diff --check
npm run build
cargo test --quiet --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --debug --no-bundle
```

## 必须继续检查的问题

### Android 原生代码

本机没有 Java、Android SDK/NDK 和 Rust Android target，因此 Kotlin 尚未真实编译。必须在具备环境后运行 Android 构建并修复可能的 API/类型问题，重点检查：

- Tauri `@PermissionCallback` 方法签名；
- `ScheduleArgs` 中 `Array<Int>`/`Array<String>` 的反序列化；
- Manifest merge；
- Android 12+ 精确闹钟授权行为；
- PendingIntent 与通知小图标；
- 冷启动和已运行 Activity 接收通知 Intent 的差异；
- Google Play 对 `SCHEDULE_EXACT_ALARM` 的政策资格。

### iOS 原生代码

本机没有 Xcode/Swift 移动构建环境，必须检查：

- `Plugin, UNUserNotificationCenterDelegate` 继承和 delegate 生命周期；
- `invoke.resolve(["targetDate": date as Any])` 对 `nil` 的序列化；
- 冷启动通知点击是否在插件设置 delegate 前丢失；
- 通知队列添加失败时的错误返回；
- iOS 权限撤销后的同步状态。

### 产品一致性

- “数据”区域目前只有本地说明，尚未把 Phase 4 备份验收 UI 正式产品化。
- 精确提醒系统设置返回后应主动刷新 `ReminderStatus`。
- 通知权限被系统设置撤销后，应在应用恢复前台时刷新状态。
- 主题失败回滚还可进一步收紧；提醒失败已做原生计划回滚尝试。
- 当前深色主题通过受控 CSS 覆盖现有 Tailwind 色类，真机仍需逐屏检查对比度。
- 动态字体、TalkBack、VoiceOver、200% 字号、小屏和横屏均未真机验收。
- `navigator.vibrate(15)` 是否在最终 Android WebView/清单中生效需要真机确认。

## 当前工作区说明

工作区在 Phase 5 开始前已经包含 Phase 1–4 的未提交连续开发改动。不要重置、覆盖或格式化全仓；继续保留所有现有改动。

Phase 5 新增或重点修改文件：

```text
CODEX_MASTER_SPEC.md
PHASE5_PROGRESS.md
package.json
package-lock.json
src/settings.ts
src/SettingsView.tsx
src/main.tsx
src/App.tsx
src/App.css
src-tauri/Cargo.toml
src-tauri/Cargo.lock
src-tauri/src/lib.rs
src-tauri/capabilities/default.json
src-tauri/capabilities/mobile.json
plugins/gougou-reminder/**
```

## 当前完成度结论

Phase 5 的设计、Rust 设置层、前端设置/锁屏、Android/iOS 提醒主体实现、通知目标应用层接线和桌面可验证构建已经完成。

当前达到 `Phase 5 Local Experience Ready for Device Review`。Android/iOS 原生代码尚未在本机编译，通知点击、提醒权限、生物识别、动态字体和无障碍等真机矩阵仍未完成，因此不能称为商店发布就绪。
