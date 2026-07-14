# Phase 6 当前进度记录

更新时间：2026-07-14（Asia/Shanghai）

## 仓库基线

- `CODEX_MASTER_SPEC.md` 已更新到 v1.7：Phase 6 当前检查点和剩余执行顺序已与本记录对齐，Phase 7 继续专门处理性能、内存、包体和发布工程。
- Phase 1–5 连续改动已提交为 `d4485b9`，并推送到 `origin/main`。
- 本文记录的 Phase 6 Android 工程、真机修复和验收结果尚未提交或推送。

当前未提交范围：

- 已修改：Android reminder 插件、`src/App.tsx`、`src/EditorView.tsx`、`src/main.tsx`、`src/SettingsView.tsx`。
- 新增：`PHASE6_PROGRESS.md`、Tauri 生成的 `src-tauri/gen/` Android 工程。
- 工作区中没有需要丢弃的已知用户改动；继续工作时不得 reset 或覆盖这些变更。

## Android 环境

- 用户级 Temurin JDK 17：`~/.local/share/gougou-android/jdk`。
- Android SDK：`~/.local/share/gougou-android/sdk`。
- 已安装 Android API 36、Build Tools 36.0.0、Platform Tools、NDK 29.0.14206865。
- 已安装 Rust Android targets：`aarch64`、`armv7`、`i686`、`x86_64`。
- Tauri Android 工程位于 `src-tauri/gen/android/`。
- Gradle 8.14.3 官方分发包已单独下载并通过官方 SHA-256 校验，缓存于 `~/.local/share/gougou-android/gradle/`。
- Gradle wrapper 当前已恢复官方 `https://services.gradle.org/` 分发地址，网络超时为 120 秒；构建时可临时使用已校验的本地缓存，完成后必须恢复官方地址。

## 真机信息

- 设备：vivo V2337A / PD2337。
- 系统：Android 15 / API 35。
- ABI：`arm64-v8a`。
- 应用 ID：`com.ym0523lv.gougou`，版本 `0.1.0`。
- 无线 ADB 端口在锁屏后可能失效或变化；最后一次成功连接端口为 `192.168.2.43:46251`，继续前仍需确认 transport 为 `device`。
- vivo 的“后台耗电管理”已为 Gougou 选择“允许后台高耗电”。这个设置必须保留，否则系统会以 `Reason=frozen` 冻结应用并移除提醒闹钟。
- 当前收尾状态：通知权限允许、精确提醒保持启用、隐私锁在完成关闭认证后为关闭；2026-07-14 日记保留文字、勾选和一张已持久化图片。

## 已完成的真机验收

### 基础功能

- arm64 debug APK 已通过无线 ADB 覆盖安装，冷启动无崩溃。
- 首屏月历、日期选择、打勾、触感反馈基础路径正常。
- 编辑器输入、自动保存、返回和重新打开后的内容持久化正常。
- 设置页四个区域和返回月历正常。
- 当前测试数据中 2026-07-14 已打勾，因此后续提醒会正确跳过该日期。

### 大约时间提醒

- Android 15 通知权限请求和授权正常。
- `RTC_WAKEUP` 大约时间闹钟可以注册。
- 第一次锁屏测试失败的原因已定位为 vivo 后台冻结，不是 Reminder Receiver 或通知权限错误。
- 允许后台高耗电后，闹钟保持在系统队列，锁屏通知成功显示。
- 用户关闭了“来消息亮屏”，所以通知不会主动点亮屏幕；这只影响展示方式，不影响送达。

### 精确提醒

- 开启“尽量准时”后，vivo 没有额外弹出系统授权页，但系统已将 Gougou 识别为允许项。
- 22:00 闹钟已由 ADB 确认为 `RTC_WAKEUP`、`window=0`、`exactAllowReason=allow-listed`。
- 22:00 锁屏状态准时触发通知，ADB 在 22:00:09 确认通知已存在。
- Receiver 触发后正确重排下一次提醒，并因 14 日已打勾而跳到 15 日 22:00；下一次仍为零窗口精确闹钟。

### 通知点击目标日期

- 真机发现：应用已在后台运行时点击通知会回到应用，但仍保留原来选择的 14 日。
- 已修复 Android `onNewIntent`：新通知日期先保存到原生偏好设置，前端恢复可见时再通过 Rust 命令消费。
- App 现在把每次通知视为新的导航请求，切回月历并选择目标日期；同一天的重复通知也不会因 React state 相同而丢失。
- 已用 ADB Intent 和截图验证热启动路径：后台 Activity 收到目标 14 日后正确选中 14 日。
- 已用 `am kill` 后的冷启动 Intent 和截图验证进程回收路径：目标 13 日正确选中 13 日。
- 用户点击真实 22:00 通知后，应用正确选中 13 日。

### 通知权限撤销与恢复

- 最新修复 APK 已使用 `-r -t --no-streaming` 覆盖安装，首次安装时间、14 日勾选状态和 reminder SharedPreferences 均保留。
- 系统关闭通知后，`POST_NOTIFICATIONS` 为 `granted=false`，设置页显示“系统通知权限已关闭，可在系统设置中恢复”，缓存更新为 `scheduled=false`，活动 Gougou 闹钟为零。
- 系统恢复通知并真正切回 Gougou 后，缓存更新为 `lastNotificationsAllowed=true`、`scheduled=true`，设置页恢复为“系统已允许尽量准时”。
- 恢复后下一次提醒为 2026-07-15 22:02，ADB 确认为 `RTC_WAKEUP`、`window=0`、`exactAllowReason=allow-listed`。
- 真机发现 vivo 的权限状态可能晚于原生 `onResume` 传播，Android WebView 从系统设置返回也不保证触发 `visibilitychange`。已将系统状态变化协调复用于 `getStatus`，并在应用级与设置页同时监听 `window.focus`；月历、设置页和冷启动均能完成协调。
- ADB 启动系统设置会产生独立或嵌套任务栈；验收时必须以 `topResumedActivity` 确认为 Gougou 后再判断，不能把系统设置或其他前台应用中的未协调状态记为失败。

### 隐私锁和后台 30 秒重锁

- 开启隐私锁会启动系统 `BiometricActivity`，成功认证后开关保存为开启。
- 返回桌面 31 秒后重新进入，应用先显示 Gougou 锁定页，月历和记录摘要均不可见。
- 在解锁认证中取消后仍停留在锁定状态；成功认证后恢复月历，2026-07-14 的勾选数据保持不变。
- 关闭隐私锁同样会启动系统认证；取消认证后应用保持锁定，重新认证成功后开关才关闭。
- 未自动制造连续生物识别失败或临时锁定，以免影响设备正常解锁；设备凭据回退和失败次数边界仍需持机人工验证。

### 图片导入与显示

- Android 系统图片选择器可返回媒体，原图和 `.thumb.webp` 缩略图均已复制到 Gougou 应用沙盒的 `assets/` 目录。
- 真机发现前端硬编码的 `gougou-asset://localhost/` 在 Android WebView 中显示为破损图片；已改用 Tauri `convertFileSrc` 生成平台对应的受控协议 URL。
- 修复 APK 覆盖安装并重启应用后，2026-07-14 日记中的图片正常显示，原有文字、勾选和图片引用均保持。
- 编辑器底栏七个按钮改为等宽弹性布局，字号使用 `clamp(0.75rem, 3.5vw, 0.875rem)` 按屏幕宽度自适应；当前 1172×2748 设备上七个中文标签均完整横排，保留 44px 最小触控高度。
- 删除图片引用后的安全回收和重新插入仍待继续验证。

## 最新已安装修复

以下内容已完成代码修改、Android Gradle/Kotlin 编译、覆盖安装和对应真机回归：

- Android 会区分通知权限的首次可请求状态和用户已撤销状态，撤销后返回 `denied`，不再误报为 `prompt`。
- 应用恢复前台时会比较通知权限和精确闹钟授权的实际变化。
- 通知权限被撤销时立即取消已注册闹钟；权限恢复时按现有设置重新注册。
- 精确闹钟授权发生变化时自动在精确与大约时间提醒之间重排。
- 设置页从系统权限页返回时会刷新实际提醒状态。
- Android 图片预览使用 Tauri 跨平台自定义协议 URL，不再硬编码桌面协议格式。
- 编辑器底栏文字使用等宽弹性按钮和流式字号，不再逐字换行或截断“重做”。

最新已安装 APK：

```text
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

该 APK 当前大小为 382,904,413 bytes，`aapt` 确认只包含 `arm64-v8a`，已通过 Kotlin/Gradle 编译并安装。后续覆盖安装仍必须使用 `-r -t` 以保留应用数据和调试包资格。

## 已通过的自动检查

- `git diff --check`。
- `npm run build`，TypeScript 和 Vite 生产构建通过。
- `cargo fmt --all -- --check`。
- `cargo test --quiet`：15 个 Rust 测试通过。
- Android arm64 debug APK 多次完成真实 Gradle 构建；Gougou Kotlin 代码无编译错误。
- 2026-07-14 最新 APK 的 SHA-256 为 `045ad7d4c706047a757a7599f876aa8d3f6f6c6e3de1302fe5e6b7667ec38d8c`；Gradle wrapper 已恢复官方分发地址。
- 合并 Manifest 已确认包含通知、重启、精确闹钟、生物识别权限和两个 Reminder Receiver。
- 当前仅有 Tauri/Gradle 生成代码的弃用提示，以及已知的 Vite 大 chunk 提示。

## 当前结论与下一执行点

- Phase 6 尚未完成，不能输出 `Phase 6 Device Matrix Ready for Optimization`，也不能进入 Release Candidate 状态。
- 当前没有需要继续修改的已知通知或隐私锁代码；下一项是完成图片引用删除与安全回收闭环。

1. 删除当前日记中的图片引用，确认正文和 `entry_assets` 更新，且无引用的原图与缩略图被安全回收。
2. 重新插入图片并重启应用，复核引用重建和持久化。
3. 继续备份导出、篡改包拒绝、有效恢复和恢复一致性矩阵。

## 接下来按顺序执行

### 1. 通知权限撤销与恢复（已完成）

1. 保持 Gougou 的“晚间提醒”和“尽量准时”开启。
2. 在系统“应用信息 → 通知”中关闭 Gougou 通知权限，再返回 Gougou 设置页。
3. 预期页面显示“系统通知权限已关闭，可在系统设置中恢复”，ADB 确认 `POST_NOTIFICATIONS` 为 denied，且 Gougou 闹钟已从队列移除。
4. 回到系统设置重新允许通知，再返回 Gougou。
5. 预期提示恢复为“系统已允许尽量准时”，ADB 确认精确闹钟重新注册为 `window=0`。

### 2. 精确闹钟授权降级与恢复（设备限制，已完成调查）

- `android.settings.REQUEST_SCHEDULE_EXACT_ALARM` 可解析到 vivo `Settings$AlarmsAndRemindersAppActivity`，系统会显示 Gougou 的“闹钟与提醒”页面。
- 页面无障碍节点显示外层开关 `enabled=false`，内部开关 `checked=true`；`SCHEDULE_EXACT_ALARM` app-op 为 `default`，实际能力由 vivo allow-list 提供。
- 该设备没有可独立撤销的精确闹钟开关，因此无法在不改变后台高耗电策略的前提下执行“精确 → 大约 → 精确”降级矩阵。
- 按验收边界保留“允许后台高耗电”，不使用厂商冻结路径冒充精确授权撤销；当前实际闹钟仍为 `window=0`、`exactAllowReason=allow-listed`。

### 3. 隐私锁和后台 30 秒重锁（自动安全路径已完成）

- 已通过：开启认证、后台 31 秒重锁、锁屏内容遮挡、取消后保持锁定、成功解锁、数据保持、关闭认证及取消保护。
- 待持机人工验证：连续失败、临时锁定和设备凭据回退；自动验收不故意触发设备级生物识别锁定。

### 4. 图片引用与安全回收（当前下一项）

1. 删除 2026-07-14 日记中的当前图片，等待自动保存并重新打开该日记。
2. 确认 Markdown 不再包含资源路径，`entry_assets` 不再引用该资源；仅当引用数为零时删除原图与 `.thumb.webp`。
3. 重新选择图片，确认新资源引用建立；覆盖安装或重启应用后图片继续显示。

### 5. 备份导出与恢复

1. 导出包含当前日记、图片和设置的有效备份，确认 ZIP 清单、结构化数据、资源及哈希齐全。
2. 制造单一可识别篡改，确认检查或恢复明确拒绝且不改变现有数据库、图片和设置。
3. 使用有效备份执行恢复，确认文字、勾选、图片引用、提醒和外观设置一致。

### 6. 提醒剩余边界

- 验证静默日、暂停一周和当天打勾后跳过提醒，并确认设置修改后原生队列立即重排。
- 精确闹钟独立降级在当前 vivo 上不可操作，维持已记录的设备限制，不关闭“允许后台高耗电”。

### 7. 外观、布局与辅助功能

- 依次验证动态字体和 200% 大字体、TalkBack、系统减少动画、浅色/深色/跟随系统。
- 验证小屏、横屏、软键盘、系统返回键、安全区和编辑器底栏；底栏当前真机自适应结果作为回归基线。

### 8. 设备重启恢复

- 验证 `ReminderBootReceiver` 在设备重启后按已启用缓存重新注册提醒。
- 重启会中断无线 ADB 并影响用户当前设备环境，执行前必须再次取得用户确认。

### 9. iOS 对等验收（当前环境阻塞）

- 需要 macOS、Xcode 和 iPhone 验证原生编译、未来 30 条通知队列、权限撤销、通知点击、LocalAuthentication、VoiceOver、安全区和生命周期恢复。
- 当前 Linux/WSL 环境不能用桌面结果替代上述真机结论。

### 10. Phase 6 收尾与 Phase 7 交接

- 按主规范 Phase 7 分别建立 release 包体、安装占用、PSS/RSS/USS、WebView renderer、冷启动和循环稳定性基线；不得把约 383 MB 的 debug APK 或用户观察到的 300 MB+ 内存数字互相替代。
- 评估 `SCHEDULE_EXACT_ALARM` 的 Google Play 政策资格和产品是否真的需要保留精确提醒。
- 检查 Release Manifest 是否应移除模板默认的 `INTERNET` 权限。
- 清理并验证 release APK/AAB 的 ABI、体积、签名和权限，不提交构建产物或本机绝对路径缓存。
- Android 矩阵通过后再整理 Phase 6 提交并推送；提交前复查 `src-tauri/gen/` 中哪些生成文件应纳入版本控制。
- 仅在 Android 矩阵通过且 iOS 项逐项通过或明确阻塞后，才可输出 `Phase 6 Device Matrix Ready for Optimization` 并进入 Phase 7。
