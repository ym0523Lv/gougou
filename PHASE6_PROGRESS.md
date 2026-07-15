# Phase 6 当前进度记录

更新时间：2026-07-15（Asia/Shanghai）

## 仓库基线

- `CODEX_MASTER_SPEC.md` 已更新到 v1.7：Phase 6 当前检查点和剩余执行顺序已与本记录对齐，Phase 7 继续专门处理性能、内存、包体和发布工程。
- Phase 1–5 连续改动已提交为 `d4485b9`；Phase 6 Android 工程、真机修复和截至 2026-07-14 的验收结果已提交为 `7c52c23`。
- `main` 与 `origin/main` 已对齐到 `7c52c23`；2026-07-15 继续完成图片回收、备份恢复和提醒边界闭环，并对 200% 字体发现的编辑器布局缺陷做了最小修复。

开始 2026-07-15 验收前工作区干净；当前未提交范围为 `src/EditorView.tsx`、`CODEX_MASTER_SPEC.md` 和 `PHASE6_PROGRESS.md`。

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
- 2026-07-15 通过真实编辑器删除 2026-07-14 的图片节点：revision 从 8 增加到 9，原文字和勾选保持，Markdown 不再包含资产路径，`entry_assets` 引用清空，零引用原图与 `.thumb.webp` 缩略图同步回收。
- 通过 Android 系统图片选择器重新插入后，revision 从 9 增加到 10，新原图、缩略图和 `entry_assets` 引用一致；强制停止并冷启动后重开同日日记，文字、勾选和图片均正常显示。

### 备份导出与恢复

- 2026-07-15 导出 `gougou-backup-20260715.zip`，ZIP 仅包含 `manifest.json`、`entries.json` 和 1 个被引用原图；format v1、资源大小、SHA-256、1 个日期、1 条引用和 10 项设置均一致。
- 仅将 manifest 中资源 SHA-256 改为全零后，`inspect_backup` 明确返回 `invalid_backup`；拒绝前后数据库逻辑摘要和原图/缩略图 SHA-256 完全相同。
- 为证明恢复非空跑，临时去除图片引用、取消勾选并把主题从 `system` 改为 `dark`；有效备份通过检查后以 `replace_all` 写入 1 个日期，恢复后数据库逻辑摘要与导出前完全相同。
- 恢复后 2026-07-14 的文字、勾选、revision 10、图片引用和原图 SHA-256 均复原，冷启动后图片正常显示；主题恢复为 `system`。
- 冷启动后原生提醒缓存与恢复设置一致：开启、22:02、尽量准时、无静默日和暂停；AlarmManager 重建为 2026-07-15 22:02 的 `RTC_WAKEUP`、`window=0`、`exactAllowReason=allow-listed`。
- 备份格式当前只携带 `entry_assets` 引用的原图，不携带派生 `.thumb.webp`；当前受控预览仍使用原图且功能正常，Phase 7 实施缩略图与懒加载时需同时确定恢复后的派生图再生成策略。

### 提醒剩余边界

- 把周三设为静默日后，下一次提醒从 2026-07-15 22:02 立即跳到 7 月 16 日 22:02；清除静默日后立即回到 7 月 15 日。
- 暂停至 2026-07-22（含当日）后，下一次提醒立即跳到 7 月 23 日 22:02；清除暂停后立即回到 7 月 15 日。
- 通过月历真实 UI 勾选 2026-07-15 后，原生 `skipDates` 包含当天，下一次提醒立即跳到 7 月 16 日 22:02；取消勾选并用既有空行清理规则删除测试行后，提醒回到 7 月 15 日。
- 上述每一次重排均为 `RTC_WAKEUP`、`window=0`、`exactAllowReason=allow-listed`；验收后数据库逻辑摘要恢复到验收前值，仅保留 2026-07-14 的文字、勾选和图片，静默日与暂停均为空。

### 外观、减少动画与 200% 字体

- 设置页深色、浅色和跟随系统三态均通过：控件值、`user_settings`、根节点 `data-theme`、`color-scheme` 和计算背景一致；验收后恢复为 `system`。
- 应用内“减少动画”开关可正确持久化并设置 `data-reduce-motion=true`，CSS 会将滚动改为 `auto`、将 transition/animation 缩短至 `0.01ms`；验收后恢复为关闭。
- 当前 vivo Android 15 / WebView 在三项系统动画缩放为 0 或厂商 `reduced_dynamic_effects=1` 时，`prefers-reduced-motion: reduce` 仍返回 `false`；这是当前设备/WebView 未映射的平台限制，不用应用内开关冒充系统结果。三项缩放已恢复 `1.0`，厂商键已恢复 `0`。
- 字体缩放设为 2.0 后，月历页根字号为 32px、无水平溢出、主要控件最小高度为 44px；但编辑器顶栏日期侵入状态栏，七个工具标签在约 46px 宽的按钮中需要约 68px 内容宽度，真机画面确认已互相覆盖。
- `EditorView` 已做最小修复：顶栏改为三列网格并允许中间日期换行，工具按钮改为按内容宽度且不收缩，超出时复用现有横向滚动。`npm run build` 和 Android arm64 debug APK 构建已通过，但新 APK 尚未覆盖安装，200% 字体真机回归留待下一次继续。
- 验收结束前系统字体缩放已恢复为 `1.0`。

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

最新已构建、待覆盖安装 APK：

```text
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

该 APK 于 2026-07-15 22:07 构建，大小为 382,904,541 bytes，SHA-256 为 `a34d68e92aeef16726bfa20afa88bb384bf957ed5f2d0c58052d5d6635ebca15`。构建时在官方分发网络超时后使用已校验的本地 Gradle 8.14.3 ZIP；构建完成后 wrapper 已恢复官方 `https://services.gradle.org/` 地址。

## 已通过的自动检查

- `git diff --check`。
- `npm run build`，TypeScript 和 Vite 生产构建通过。
- `cargo fmt --all -- --check`。
- `cargo test --quiet`：15 个 Rust 测试通过。
- 2026-07-15 重跑 `npm run build` 和 `cargo test --quiet`，结果仍通过；仅有已知的 Vite 大 chunk 提示。
- 200% 字体布局修复后再次通过 `npm run build` 和 Android arm64 debug APK 真实 Gradle 构建；仅有已知的 Vite 大 chunk 与 Gradle 弃用提示。
- Android arm64 debug APK 多次完成真实 Gradle 构建；Gougou Kotlin 代码无编译错误。
- 2026-07-14 最新 APK 的 SHA-256 为 `045ad7d4c706047a757a7599f876aa8d3f6f6c6e3de1302fe5e6b7667ec38d8c`；Gradle wrapper 已恢复官方分发地址。
- 合并 Manifest 已确认包含通知、重启、精确闹钟、生物识别权限和两个 Reminder Receiver。
- 当前仅有 Tauri/Gradle 生成代码的弃用提示，以及已知的 Vite 大 chunk 提示。

## 当前结论与下一执行点

- Phase 6 尚未完成，不能输出 `Phase 6 Device Matrix Ready for Optimization`，也不能进入 Release Candidate 状态。
- 当前没有需要继续修改的已知通知、隐私锁、图片回收、备份恢复或提醒边界代码；下一项是覆盖安装新 APK 并回归 200% 字体修复。

1. 使用 `-r -t --no-streaming` 覆盖安装 SHA-256 为 `a34d68e9…ca15` 的新 APK，确认原有日记、图片、勾选与提醒缓存保留。
2. 重新将字体设为 2.0，复核编辑器日期顶栏不侵入状态栏，七个工具标签不覆盖、可横向滚动且触控高度不小于 44px；完成后再恢复字体 1.0。
3. 继续小屏、横屏、软键盘、系统返回键、安全区和 TalkBack 真实语音/焦点顺序验收。

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

### 4. 图片引用与安全回收（已完成）

1. 删除 2026-07-14 日记中的当前图片，等待自动保存并重新打开该日记。
2. 确认 Markdown 不再包含资源路径，`entry_assets` 不再引用该资源；仅当引用数为零时删除原图与 `.thumb.webp`。
3. 重新选择图片，确认新资源引用建立；覆盖安装或重启应用后图片继续显示。

上述三项已于 2026-07-15 在 vivo V2337A / Android 15 通过；本轮未发现需要修复的代码缺陷。

### 5. 备份导出与恢复（已完成）

1. 导出包含当前日记、图片和设置的有效备份，确认 ZIP 清单、结构化数据、资源及哈希齐全。
2. 制造单一可识别篡改，确认检查或恢复明确拒绝且不改变现有数据库、图片和设置。
3. 使用有效备份执行恢复，确认文字、勾选、图片引用、提醒和外观设置一致。

上述三项已于 2026-07-15 在 vivo V2337A / Android 15 通过；本轮未发现需要修复的代码缺陷。

### 6. 提醒剩余边界（已完成）

- 验证静默日、暂停一周和当天打勾后跳过提醒，并确认设置修改后原生队列立即重排。
- 精确闹钟独立降级在当前 vivo 上不可操作，维持已记录的设备限制，不关闭“允许后台高耗电”。

静默日、暂停和当天打勾跳过已于 2026-07-15 在 vivo V2337A / Android 15 通过；本轮未发现需要修复的代码缺陷。

### 7. 外观、布局与辅助功能（当前下一项）

- 依次验证动态字体和 200% 大字体、TalkBack、系统减少动画、浅色/深色/跟随系统。
- 验证小屏、横屏、软键盘、系统返回键、安全区和编辑器底栏；底栏当前真机自适应结果作为回归基线。

主题三态和应用内减少动画已通过；系统减少动画在当前 WebView 上未映射媒体查询，已记录设备限制。200% 字体发现的编辑器顶栏和工具栏缺陷已做最小修复并通过 APK 构建，尚待覆盖安装后的真机回归。

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
