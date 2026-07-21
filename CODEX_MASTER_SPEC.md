# GOUGOU（勾勾）主工程规范（v1.7）

> 目标平台：Android（首发）与 iOS（同等架构准备）  
> 技术栈：Tauri v2 + React 18 + TypeScript + TailwindCSS + Rust + rusqlite  
> 产品定位：极简、反焦虑、纯本地优先的移动端日记

---

## 1. 产品北极星

“勾勾”来自“勾勾手指”的温柔约定。它不要求用户每天完成一篇日记，只帮助用户留下一个不带负担的存在证明。

核心问题是传统日记的“愧疚与摩擦失败”：疲惫、低落或忙碌的人写不出长文，看到空白日历又产生羞耻，最终弃用产品。

### 双层记录模型

1. **第一层：0.1 秒打勾**
   - `is_ticked` 是独立布尔状态。
   - 它表示“今天没有白过；我活过今天”，不要求解释。

2. **第二层：可选倾诉画布**
   - 用户愿意时才打开块编辑器写 Markdown。
   - 写文字不要求打勾；打勾也不要求写文字。

### 心理学红线

- 不显示“漏打卡天数”“断签”“落后于目标”等羞耻化表达。
- 不把连续天数设计为主指标，不以红色、警告或倒计时催促用户。
- 提醒必须由用户主动开启，可随时暂停；文案只能邀请，不能责备。
- 月度摘要只能陈述已发生的正向事实，例如“本月留下了 8 个勾勾”。

---

## 2. 产品范围

### v1 必须具备

- 月历选择日期、打勾和查看当天是否有文字。
- 目标日期锁定的 Markdown 块编辑器。
- 自动保存、低打扰的失败提示和恢复能力。
- 图片插入、本地引用管理和安全回收。
- 本地通知提醒、隐私锁、导出与导入。
- Android 与 iOS 的安全区、软键盘、返回手势和动态字体适配。

### v1 明确不做

- 账号、登录、REST API、云同步、Firebase、Supabase、埋点上传。
- 社交、排行榜、连续打卡竞赛、广告和付费墙。
- 未经用户启用的推送、后台联网和跨设备自动同步。
- 把 Markdown、图片或数据库内容上传到任何服务。

---

## 3. 技术与架构铁律

### 3.1 本地优先与最小权限

1. 所有业务数据只存在 OS 应用沙盒内。
2. Rust 使用 `tauri::Manager::path().app_data_dir()` 动态解析根目录，禁止硬编码系统路径。
3. 标准路径如下：
   - 数据库：`{app_data_dir}/db/gougou.db`
   - 图片：`{app_data_dir}/assets/{uuid}.{ext}`
   - 临时文件：`{app_data_dir}/tmp/`
4. Rust 文件读写使用 `std::fs` 或 `tokio::fs`；不因后端文件操作而向前端开放任意文件系统权限。
5. Tauri capability 和移动端权限按最小集配置，所有 IPC 参数均在 Rust 端校验。

### 3.2 数据库所有权、迁移与并发

1. Rust 是 SQLite 的唯一写入所有者；Kotlin、Swift 和前端不得各自直接修改数据库。
2. 使用 `PRAGMA user_version` 管理数据库迁移。已发布版本只能追加迁移，禁止删除重建用户数据库。
3. 数据库连接由受控共享状态串行访问；不得持有数据库锁跨越 `await`。
4. `toggle_tick` 只修改勾选字段；正文保存只修改正文、字数和版本字段，防止互相覆盖。
5. 编辑保存携带单调递增的 `revision`。后端拒绝旧版本覆盖新版本，前端仅接受对应或更新的保存确认。
6. 为 SQLite 配置合理的 busy timeout。图片复制、压缩、目录扫描和 ZIP 操作必须在数据库锁外执行。

### 3.3 日期与稀疏记录

1. `entry_date` 是用户当前时区下的民用日期，格式固定为 `YYYY-MM-DD`；它不是 UTC 时间戳。
2. 打开编辑器时传入并锁定 `targetDate`。跨零点保存时仍写入原选日期。
3. 前端禁止把 `YYYY-MM-DD` 直接用 UTC `Date` 解析，避免跨时区偏移一天。
4. `toggle_tick(date)` 必须使用 UPSERT。日期首次被勾选时创建行，生成 UUID、创建时间和更新时间。
5. 若某行未打勾且正文为空，保存流程可删除该空行；删除前必须先完成关联图片引用的安全清理。
6. 所有日期和月份 IPC 参数在 Rust 端严格校验；月份查询使用 `[month_start, next_month_start)` 范围，而非模糊匹配。

### 3.4 自动保存与移动端生命周期

1. 不提供手动“保存”按钮。编辑器停输入 1500ms 后触发常规保存。
2. `visibilitychange`、`pagehide` 与原生生命周期信号触发立即 flush；这属于尽力保存，不得声称能在 OS 强杀前绝对同步完成。
3. 原生暂停事件不能阻塞等待 WebView IPC。可靠性由短防抖、版本控制、立即 flush 与启动恢复共同保障。
4. 保存失败时展示非阻塞状态和重试入口；离开编辑页前如存在未确认写入，先完成或明确保留待重试状态。
5. 需要原生生命周期桥接时，使用受控 Tauri 移动插件向前端发送事件，不让前端猜测后台状态。

### 3.5 图片沙盒与引用完整性

1. 图片只能通过系统媒体选择器进入应用。Android 的 `content://` 和 iOS 受限资源不得假设为普通文件路径。
2. 选择器先将媒体复制到应用可访问位置；Rust 再校验 MIME、文件魔数、大小、扩展名并改名为 UUID。
3. Markdown 只保存相对路径，如 `assets/a1b2c3d4-....jpg`；渲染时通过受控资产协议转换为可显示 URL，禁止任意 `file://`。
4. 使用 `entry_assets` 保存日记与资源的引用关系。保存正文时，在同一数据库事务内更新该关系。
5. 回收只删除数据库中没有任何引用的 UUID 资源；禁止依据“当前一篇 Markdown 未引用”删除全目录文件。
6. 不将 base64 图片写入 Markdown 或 IPC。后续实现必须有单图大小、总量、解码尺寸、缩略图和懒加载限制。
7. 临时资源位于 `tmp/`；应用启动和保存完成后可清理过期临时文件，不得删除仍在编辑缓冲中等待保存的资源。

### 3.6 Markdown 边界

1. Tiptap 是无头编辑器，必须配置确定的 Markdown 序列化与反序列化链路。
2. v1 仅承诺无损支持：段落、一级至三级标题、粗体、斜体、无序列表、有序列表、待办列表、引用、代码块、图片和换行。
3. 不在承诺集合内的复杂 Markdown 语法不得静默破坏；导入时应保守展示或明确提示。
4. `word_count` 定义为：中文、日文、韩文按可见字符计数；连续拉丁字母或数字按词计数；Markdown 语法符号和图片路径不计入。

### 3.7 本地提醒的真实性约束

1. 提醒能力完全本地化，不使用远程推送、账号或服务器。
2. 默认提醒为“晚上约 22:00”，不是跨平台绝对准点承诺。
3. Android 默认使用不精确闹钟；用户主动开启“尽量准时”后，才引导申请精确闹钟特殊权限。
4. Android 在设备重启后由原生接收器重设提醒；通知权限被撤销、精确闹钟授权被撤销时，提醒状态必须同步更新。
5. iOS 预排未来 30 天的单次本地通知。用户当天打勾后取消对应通知；每次启动应用补齐队列。长期不打开应用导致队列耗尽时，提醒自然停止。
6. 通知点击打开对应日期的应用界面。v1 不允许原生通知组件绕过 Rust 直接写 SQLite。
7. 提醒由 `gougou-reminder` 原生 Tauri 插件承载：Android 为 Kotlin，iOS 为 Swift。插件只管理权限、调度和深链接，不拥有日记数据写入权。

---

## 4. 数据库模型

首次安装创建以下结构；后续变更必须通过迁移完成。

```sql
CREATE TABLE IF NOT EXISTS entries (
    id TEXT PRIMARY KEY,
    entry_date TEXT UNIQUE NOT NULL,
    is_ticked INTEGER NOT NULL DEFAULT 0 CHECK (is_ticked IN (0, 1)),
    content_md TEXT NOT NULL DEFAULT '',
    word_count INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entry_assets (
    entry_id TEXT NOT NULL,
    asset_name TEXT NOT NULL,
    PRIMARY KEY (entry_id, asset_name),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS user_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entries_entry_date ON entries(entry_date);
CREATE INDEX IF NOT EXISTS idx_entry_assets_asset_name ON entry_assets(asset_name);
```

### 数据传输对象

- `MonthEntrySummary`：`entry_date`、`is_ticked`、`has_content`、`updated_at`。月历接口不得返回整月 `content_md`。
- `EntryDetail`：编辑器单日读取时才返回 Markdown、字数、版本与资源信息。
- 所有 Rust IPC 返回可序列化的明确错误；不得把底层数据库路径、完整 SQL 或私密内容暴露给界面。

---

## 5. 交互与界面规范

### 5.1 CalendarHome

1. 使用周一为首日的 `7x6` 月历。主内容可用空间随安全区和 Dock 自适应，禁止固定占视口 75%。
2. 今日日期有低干扰的点状提示；已打勾和有文字使用可区分且色盲友好的状态。
3. 选择某日后，高亮该日期，底部 Dock 同步显示该日状态。
4. Dock 固定在底部，包含：
   - 左侧：`打个勾` 或 `已打勾`，点击只调用 `toggle_tick`，不导航。
   - 右侧：`写几句`，进入该日期的编辑器。
5. Dock、月历内容与安全区之间必须留出明确间距；主内容的底部内边距至少等于 Dock 高度加 `safe-area-inset-bottom`。
6. 小屏、横屏和动态字体下，日期格与主要按钮的可触达面积不小于 44px；无法容纳时主区域可滚动，不压缩到不可用。

### 5.2 EditorView

1. 顶栏：返回、锁定日期、导出菜单。
2. 底栏：粗体、标题、列表、待办、图片、撤销、重做。触碰格式按钮时必须保持编辑选区。
3. 软键盘出现时，格式栏位于可视视口底部上方；使用 `100dvh`、显式安全区 CSS 和 `visualViewport` 适配，不依赖不存在的 Tailwind 默认安全区类。
4. Android 系统返回和 iOS 返回手势都遵循应用路由：编辑页返回月历，不得意外退出应用。
5. 导出仅导出当前日记为 Markdown 和其引用资源；完整备份从设置页进入。

### 5.3 设置与辅助界面

1. 设置分为：提醒、隐私、数据、外观与辅助功能。
2. 隐私锁使用 Android `BiometricPrompt` 与 iOS `LocalAuthentication`。它是应用访问门槛，不得宣称为独立数据库加密。
3. 支持系统/浅色/深色、减少动画、触感反馈开关与动态字体。
4. 所有图标按钮提供可访问名称；状态不只依赖颜色表达。

---

## 6. 晚间提醒产品规范

### 默认行为

- 默认关闭。用户在设置页主动开启后，才解释用途并请求系统通知权限。
- 默认时间为 22:00，界面文案为“晚上约 10 点”。
- 仅在当天尚未打勾时才保留提醒。
- 推荐文案：

  ```text
  今天还没有留下一个勾勾。
  要不要用 0.1 秒，和今天打个招呼？
  ```

- 禁止使用“忘记”“断签”“连续记录”“补卡”等措辞。
- 用户可关闭提醒、修改时间、设置静默日，或选择暂时暂停一周。

### 权限与降级

- Android 13 及以上：请求 `POST_NOTIFICATIONS` 运行时权限。
- iOS：请求本地通知授权。
- Android 默认不申请 `SCHEDULE_EXACT_ALARM`。若用户开启“尽量准时”，先说明耗电和系统特殊授权，再跳转系统设置；未获授权时回退到近似提醒。
- Android 重启后重排提醒需要 `RECEIVE_BOOT_COMPLETED`；应用首次启动前不处理此能力。
- 用户拒绝、关闭或撤销权限时不反复弹窗催促，只在设置页呈现可恢复状态。

---

## 7. 导入、导出与隐私

### 完整备份

导出包格式为 `gougou-backup-YYYYMMDD.zip`，至少包含：

```text
manifest.json        # 格式版本、导出时间、校验信息
entries.json         # 日记与设置的结构化数据
assets/              # 仅被引用的图片
```

### 导入规则

1. 在临时目录验证 ZIP、清单版本、资源哈希和数据格式。
2. 先展示导入模式：合并较新内容或替换本地数据。
3. 合并以日期为冲突单位，默认保留 `updated_at` 较新的条目；替换操作必须二次确认。
4. 验证成功后再写入数据库与资源目录；任何失败不得破坏既有数据。
5. 全程使用系统文件选择/分享能力，不获取用户任意目录长期权限。

---

## 8. 验收与测试基线

- 数据库首次创建、升级迁移、重启持久化、非法 IPC 输入。
- 首次勾选、反复勾选、月度读取、跨月读取与跨零点编辑。
- 自动保存乱序、后台切换、应用被系统回收后的已保存内容恢复。
- 图片多日引用、删除引用后的回收、异常中断后的临时文件清理。
- 通知权限允许/拒绝/撤销、Android 重启、iOS 30 天队列取消与补齐。
- 导出后导入、损坏备份、合并冲突和替换失败回滚。
- Android 小屏/横屏/刘海屏、iPhone 安全区、软键盘、动态字体、深浅色和无障碍。
- 性能数据必须区分 arm64 APK/AAB 体积、安装占用、运行时 PSS/RSS/USS 和系统 WebView/缓存开销；禁止用单一“占用”数字替代全部指标。
- 性能与体积优化必须先记录同设备、同构建类型、同场景的基线，再做单变量改动和前后对比；debug 构建不得作为 release 体积结论。

---

## 9. 分期执行

### PHASE 1：脚手架与数据库基础（已完成）

1. 初始化标准 Tauri v2 + React + TypeScript + TailwindCSS 工程结构。
2. 实现 `src-tauri/src/db.rs`：启动时解析 `app_data_dir()`，创建数据库目录、执行首次建表与版本初始化。
3. 暴露两个 IPC 命令：
   - `get_month_entries(year_month: String) -> Result<Vec<MonthEntrySummary>, String>`
   - `toggle_tick(date: String) -> Result<bool, String>`
4. 实现 CalendarHome：当前月份 `7x6` 网格、日期选择、今日提示、已勾选视觉状态和底部 Dock。
5. Dock 左侧调用 `toggle_tick` 并立即更新对应单元格；不实现页面跳转以外的编辑器功能。
6. 验证首次勾选、反复切换、重启持久化、月份读取和类型/Rust 检查。

### PHASE 1 禁止提前实现

- EditorView、Tiptap、Markdown 保存、图片、提醒、隐私锁、导入导出、搜索、云端能力。

### PHASE 1 结束条件

完成上述范围并通过针对性验证后立即停止，输出：`Phase 1 Scaffold Ready for Review`。

### PHASE 2：文本编辑与可靠保存（已实现，待完整端到端验收）

#### 范围与边界

1. 从 CalendarHome 的“写几句”进入 EditorView，并将所选 `targetDate` 以 `YYYY-MM-DD` 字符串锁定在编辑会话内；跨零点、切换月份或界面重渲染不得改变它。
2. 使用 Tiptap React 与官方 `@tiptap/markdown` 配置确定的 Markdown 双向链路。仅启用并验证：段落、一级至三级标题、粗体、斜体、无序/有序列表、待办列表、引用、代码块和换行。
3. 仅实现文本编辑。本阶段不安装媒体选择器、不写入 `entry_assets`、不启用图片插入或受控资源协议；图片功能保留到 PHASE 3。
4. EditorView 提供返回、锁定日期、粗体、标题、列表、待办、撤销和重做。格式按钮必须在 `pointerdown` 阶段阻止自身夺取焦点，从而保持编辑选区。图片与导出菜单不在本阶段呈现。
5. 编辑器停止输入 1500ms 后自动保存；`visibilitychange` 与 `pagehide` 尝试立即 flush。界面仅承诺尽力保存，不对系统强杀作绝对保证。
6. 保存中、已保存和失败状态必须非阻塞可访问；失败时保留编辑缓冲并提供“重试”。离开 EditorView 时，若仍有未确认内容，先触发一次 flush；失败后仍允许返回，缓冲在当前应用会话内按日期保留，重新打开同日可重试，成功确认后清除。

#### IPC 与数据一致性

1. 新增 `get_entry_detail(date: String) -> Result<EntryDetail, CommandError>`。不存在的日期返回空 Markdown、字数 `0`、版本 `0`，不得为阅读而创建行。
2. 新增 `save_entry(date: String, content_md: String, expected_revision: i64) -> Result<EntryDetail, CommandError>`。Rust 严格校验日期、版本非负与正文最大 `200_000` UTF-8 字节；字数由 Rust 根据 Markdown 可见文本计算，前端不得传入或写入 `word_count`。
3. 保存采用条件写：已有条目仅当 `revision = expected_revision` 时更新正文、字数、版本和更新时间；首次文本保存仅允许 `expected_revision = 0`。不匹配时返回稳定的 `revision_conflict` 错误，绝不以旧内容覆盖新内容。
4. `toggle_tick` 与正文保存只更新各自拥有的字段。正文变为空且未打勾时可删除空行；PHASE 2 尚无资源引用，因此删除不触及资源目录。
5. `EntryDetail` 至少返回 `exists`、`entry_date`、`content_md`、`word_count`、`revision`、`is_ticked` 与 `updated_at`。非删除保存确认的版本必须大于请求版本；空白未打勾行被删除时返回 `exists: false, revision: 0`，前端据此重置下一次新建保存的版本。前端仅接受当前请求或更新请求的确认。

#### 验收

1. 进入任意日期后读取对应正文，跨零点保存仍写回原日期；返回月历后该日显示“有文字”状态。
2. 连续输入只在停顿约 1500ms 后保存；页面隐藏与返回触发立即 flush；失败提示不遮挡编辑且可重试。
3. 旧 `expected_revision` 被拒绝，较新的已确认内容不被覆盖；打勾与正文保存互不丢失对方字段。
4. 空白未打勾条目被安全删除；空白但已打勾条目保留。
5. Rust 单元测试覆盖日期锁定相关 IPC 参数、版本冲突、空行清理和字数计算；前端类型检查与 Tauri 调试构建通过。

#### PHASE 2 禁止提前实现

- 图片、系统媒体/文件选择器、`entry_assets` 写入、资产协议、导出、提醒、隐私锁、主题设置、导入导出、云端能力。

#### PHASE 2 结束条件

完成上述范围并通过针对性验证后立即停止，输出：`Phase 2 Editor Ready for Review`。

#### PHASE 2 验收执行清单

1. Rust：运行 `cargo test`，覆盖非法日期、版本冲突、空行删除、已打勾空条目保留和字数计算。
2. 前端：运行 `npm run build` 与无打包 Tauri 调试构建，确认 TypeScript、Tiptap Markdown 序列化和 Rust IPC 可共同编译。
3. 手动：选择非当日日期后进入编辑器，跨越零点再保存，返回月历后必须仍更新原 `targetDate`。
4. 手动：连续输入后观察约 1500ms 保存；输入后立即切换后台、触发 `pagehide`、返回月历，重新打开确认已保存内容不倒退。
5. 手动：人为让保存请求返回失败，确认编辑内容保留、显示非阻塞重试入口、重新进入同日仍可继续重试。
6. Markdown：对承诺的标题、粗体、斜体、列表、待办、引用、代码块和换行进行“输入 → 保存 → 重开”往返检查；不承诺的复杂语法不得静默改写。

### 后续阶段

### PHASE 3：图片资产与引用完整性（部分实现，待验收收尾）

#### 范围与边界

1. 使用官方 Tauri dialog 插件的系统图片选择器。前端只可请求 `pickerMode: "image"`、`fileAccessMode: "copy"` 和 PNG/JPEG/WebP 过滤条件；不启用前端文件系统插件或任意路径读取能力。
2. 图片选择器先将资源复制至应用沙盒，前端仅将该受控路径传给 `import_image`。Rust 校验路径位于应用临时目录、文件大小、魔数、MIME、扩展名和解码尺寸后，才复制为 `{app_data_dir}/assets/{uuid}.{ext}`。
3. v1 图片上限：单图 10 MiB、解码像素不超过 20 MP、任一边不超过 4096 px。导入成功后生成最长边 512 px 的 WebP 缩略图，命名为 `{uuid}.thumb.webp`；图片复制、解码与缩略图生成不得持有数据库锁。
4. Markdown 仅写入 `assets/{uuid}.{ext}` 相对路径。Tiptap 图片节点只能插入由 `import_image` 返回的相对路径；不接受 base64、`file://`、网络 URL 或其他本地路径。
5. 注册只读 `gougou-asset://` 受控协议：仅能解析符合 UUID 文件名规则的应用资产。编辑器预览将相对路径转换为该协议 URL，前端不得构造任意文件路径。
6. 本阶段实现图片按钮、图片预览和删除图片 Markdown 节点；不实现图片裁剪、相册管理、OCR、网络图片、导出或完整备份。

#### 数据库与 IPC

1. `import_image(source_path: String) -> Result<AssetDetail, CommandError>` 仅接受 dialog 复制到 `{app_data_dir}/tmp/` 的暂存路径，返回 `asset_name`、`mime_type`、`width`、`height` 与受控预览 URL；所有错误不得回显原路径。
2. `save_entry` 在同一 SQLite 事务中解析 Markdown 的受限图片语法并替换 `entry_assets` 引用。任何不符合 `assets/{uuid}.{png|jpg|jpeg|webp}` 的图片路径均拒绝保存，不能静默保留。
3. 仅在事务提交后回收 `entry_assets` 已无任何引用且不在当前编辑暂存集内的原图和缩略图。应用启动时只清理超过 24 小时且未引用的暂存文件；不得扫描或删除应用沙盒外文件。
4. 正文变为空且未打勾时，先在同一事务删除其资产引用，再删除条目，最后执行安全回收。多篇日记引用同一图片时，删除其中一篇不得影响另一篇。

#### 验收

1. Android/iOS/桌面均通过系统选择器导入 PNG、JPEG 或 WebP；Android `content://` 经 copy 模式进入应用沙盒后再处理。
2. MIME、文件魔数、扩展名、大小或尺寸不符合限制的资源被拒绝，原图不会进入 `assets/`，界面只显示低打扰错误。
3. 图片 Markdown 仅含相对路径，渲染只使用 `gougou-asset://`；伪造相对路径、`file://`、网络 URL 和 base64 均被拒绝。
4. 保存、重新打开、删除图片与删除整篇未打勾日记后，`entry_assets` 与文件系统保持一致；共享资源仅在最后一个引用删除后回收。
5. Rust 单元测试覆盖受限路径、魔数、引用提取、共享引用回收与删除事务；前端类型检查及 Tauri 调试构建通过。

#### PHASE 3 禁止提前实现

- 完整备份/导入、相册浏览器、图片裁剪/OCR、网络图片、提醒、隐私锁、主题设置、云端能力。

#### PHASE 3 结束条件

完成上述范围并通过针对性验证后立即停止，输出：`Phase 3 Assets Ready for Review`。

#### PHASE 3 验收执行清单

1. Rust：为受控资产名、伪造 `file://`/网络/base64 路径、错误魔数、过大文件、过大解码尺寸、共享引用和最后引用删除补充单元测试。
2. 前端构建：运行 `npm run build` 与无打包 Tauri 调试构建，确认 dialog、图片节点和受控协议共同编译。
3. 桌面手动：依次导入 PNG、JPEG、WebP，确认 Markdown 仅保存 `assets/<uuid>.<ext>`，预览 URL 使用 `gougou-asset://`。
4. 真机手动：Android 验证 `content://` 经 dialog `copy` 模式后可导入；iOS 验证系统图片选择器和沙盒副本可导入。两端均测试拒绝/取消选择。
5. 数据一致性：让两个日期引用同一图片，删除其中一个引用后资源保留；删除最后一个引用后原图和缩略图被回收；重启后过期 `tmp/` 文件被清理。

### 后续阶段

### PHASE 4：本地备份与导入事务（已实现，待端到端验收）

#### 范围与边界

1. 完整备份始终在本地生成 `gougou-backup-YYYYMMDD.zip`，包含 `manifest.json`、`entries.json` 与仅被引用的 `assets/`；不联网、不压缩上传、不自动分享。
2. `manifest.json` 记录格式版本、导出时间、每个资源的 SHA-256 与大小。`entries.json` 仅包含 entries、entry_assets 和 user_settings 的结构化数据，不包含数据库文件或绝对路径。
3. 导出与导入入口暂以受控 IPC 实现；系统保存/打开对话框和设置页 UI 在本阶段不实现，避免提前开放通用文件访问能力。
4. 导入仅接受 `.zip`，先解压到 `{app_data_dir}/tmp/import-{uuid}`。验证 ZIP 条目数量、单条与总解压大小、路径穿越、manifest 格式版本、日期、资源名、哈希和条目引用后，才允许预览或写入。
5. 本阶段提供两种已确认的写入策略：`merge_newer` 按 `updated_at` 比较同日条目，保留较新者；`replace_all` 在数据库事务与资产暂存准备成功后替换本地内容。两者任一失败均不得破坏现有数据。

#### IPC 与一致性

1. `export_backup() -> Result<BackupExport, CommandError>` 在数据库锁内读取一致的元数据快照，在锁外读取资源并写 ZIP；若发现引用资源缺失则失败且不产生半成品备份。
2. `inspect_backup(source_path: String) -> Result<BackupPreview, CommandError>` 只验证和生成摘要，不写数据库、资产目录或设置；返回日期数、资源数、冲突数与一次性导入令牌，不返回日记正文。
3. `apply_backup(import_token: String, mode: "merge_newer" | "replace_all") -> Result<BackupImport, CommandError>` 仅接受由 `inspect_backup` 产生且未过期的令牌。资源先复制到应用临时目录，数据库更新在单一事务完成，提交后才原子移动资源并回收不再引用的旧资源。
4. 前端不得传入目标目录、SQL、数据库路径或任意 ZIP 内文件路径。所有错误使用稳定错误码，避免暴露备份绝对路径和日记内容。

#### 验收

1. 导出后可在空数据库导入并恢复 entries、settings 与被引用资源；未引用资源不进入 ZIP。
2. 损坏 ZIP、路径穿越、错误哈希、超出大小限制、未知格式版本和错误令牌均被拒绝，既有数据保持不变。
3. `merge_newer` 对同一日期只保留 `updated_at` 更新的条目；`replace_all` 需要调用端显式确认且失败可回滚。
4. Rust 测试覆盖空备份、共享资源、损坏清单、冲突合并和替换失败回滚；前端类型检查及 Tauri 调试构建通过。

#### PHASE 4 禁止提前实现

- 自动云备份、后台同步、通用文件系统能力、设置页完整 UI、提醒、隐私锁、主题设置。

#### PHASE 4 结束条件

完成上述范围并通过针对性验证后立即停止，输出：`Phase 4 Backup Ready for Review`。

#### 当前实施状态（2026-07-12）

- PHASE 1：已实现并提交。月历、SQLite 初始化、月份摘要和原子打勾命令已存在。
- PHASE 2：已实现并提交。EditorView、Markdown、1500ms 自动保存、生命周期 flush、草稿恢复、revision 条件写和 Android 系统返回键路由已存在；待补移动端端到端验收。
- PHASE 3：已实现图片选择、Android `content://` 限流复制桥、导入校验、缩略图、受控协议、引用事务和保存后回收；待补 Android 真机选择/取消、异常资源与资源回收专项测试。
- PHASE 4：已实现 export_backup、ZIP 资源 SHA-256 校验、inspect_backup/单次导入令牌、资源暂存与恢复、merge_newer、replace_all 文件系统回滚和专项单元测试；当前入口仍仅用于开发验收，待补端到端与发布交互设计。

#### PHASE 4 已完成执行顺序

1. 在 `inspect_backup` 解压到 `tmp/import-<uuid>/`，校验每个 manifest 资源的名称、大小、SHA-256 与 `entry_assets` 引用；会话保存临时目录和过期时间。
2. 在 `apply_backup` 消费会话后，先将资源复制到新的临时资产目录；任何哈希、复制或格式失败都在写数据库前退出。
3. 在单一 SQLite 事务中写 entries、settings 和选中条目的 `entry_assets`：`merge_newer` 仅替换较新的日期及其引用，`replace_all` 明确替换全部数据。
4. 数据库提交成功后才原子移动临时资产；移动失败时使用预提交数据库快照回滚，并保留诊断性但不含日记内容的错误码。
5. 提交后扫描未引用资源并回收；添加空备份、损坏 ZIP、哈希错误、共享资源、日期冲突、重复令牌、过期令牌与移动失败回滚测试。

### PHASE 5：本地提醒、隐私锁、外观与辅助功能（已实现，进入真机验收）

#### 范围与边界

1. 新增正式设置页，分为提醒、隐私、数据、外观与辅助功能。设置页只展示真实能力状态，不可用平台不得伪造成功。
2. `user_settings` 是业务设置的唯一事实来源。Android SharedPreferences 与 iOS UserDefaults 只保存由设置重建的原生调度缓存，原生代码不得读写 SQLite。
3. 本阶段不增加账号、远程推送、分析 SDK、数据库加密、应用内密码、云备份或后台网络任务。
4. Android 为首发实现；iOS 使用同一 Rust/TypeScript DTO 和插件接口，原生队列规则同时实现或明确返回 `unsupported`，不得静默失效。

#### 设置模型与 IPC

缺失键使用下列默认值，读取默认值不立即写库：

```text
reminder.enabled = false
reminder.hour = 22
reminder.minute = 0
reminder.precise = false
reminder.quiet_weekdays = []       # ISO 周一=1，周日=7
reminder.paused_until = null       # YYYY-MM-DD，含当日
privacy.lock_enabled = false
appearance.theme = system          # system | light | dark
accessibility.reduce_motion = false
accessibility.haptics = true
```

1. `get_app_settings() -> AppSettings` 一次返回完整、已校验的设置快照。
2. `update_app_settings(settings: AppSettings) -> AppSettings` 在单一 SQLite 事务中替换上述受控键；拒绝未知主题、越界时间、非法日期、重复或越界静默日。
3. 原生能力通过 `ReminderStatus` 与 `BiometricStatus` 单独返回；业务设置中的“期望启用”不得与系统权限的“实际可用”混为一谈。
4. 备份继续包含这些白名单设置。导入时未知设置键拒绝进入本地设置，防止任意键污染运行状态。

#### 本地提醒插件契约

1. 新建 `gougou-reminder` Tauri 移动插件。命令仅包含：`getStatus`、`requestPermission`、`syncSchedule`、`cancelAll`、`takeNotificationTarget`、`openBackgroundSettings`；不接收 Markdown、图片、数据库路径或 SQL。`takeNotificationTarget` 只允许单次消费通知携带的合法民用日期，`openBackgroundSettings` 只能打开系统可解析的厂商后台启动设置，不得静默修改权限。
2. `syncSchedule` 接收提醒时间、精确偏好、静默日、暂停日期、未来已打勾日期集合和通知文案。原生侧先取消旧队列，再建立可重建的新队列。
3. Android 使用单次 AlarmManager 闹钟安排下一个合格日期。默认使用不精确 API；仅在用户主动开启且系统允许时使用精确 API，否则返回 `effectivePrecise=false` 并自动降级。闹钟触发后展示本地通知并安排下一次。
4. Android 13+ 仅在用户点击开启提醒后请求 `POST_NOTIFICATIONS`。清单声明 `RECEIVE_BOOT_COMPLETED`；重启接收器只在已有启用缓存时恢复调度。
5. iOS 使用 `UNUserNotificationCenter` 预排未来 30 天的单次通知；每次启动、设置修改或打勾后取消并补齐队列。
6. 通知点击携带目标民用日期并打开应用；只导航到对应日期，不执行打勾或正文写入。
7. 用户关闭提醒时先取消原生队列，再持久化关闭状态。权限拒绝或撤销时保留设置页恢复入口，但不得循环弹窗。
8. 应用每次冷启动仍从 SQLite 设置重排提醒，作为厂商未投递 `BOOT_COMPLETED` 时的自愈路径；已验证的厂商后台启动设置仅在可解析时由用户主动打开，不引入常驻服务或私有权限写入。

#### 隐私锁

1. 使用官方 Tauri biometric 插件，对 Android 调用 `BiometricPrompt`，对 iOS 调用 `LocalAuthentication`；允许系统设备凭据作为系统级回退。
2. 开启隐私锁前必须完成一次成功验证，成功后才持久化 `privacy.lock_enabled=true`。关闭隐私锁同样需要验证。
3. 应用冷启动时若已启用锁，只渲染不含日记摘要的锁屏。应用进入后台超过 30 秒或进程重新启动后再次锁定；短暂系统选择器切换不重复打断用户。
4. 验证取消、失败或暂时锁定时保持锁屏并提供重试；不得通过错误回退展示日记内容。
5. 设置页明确说明隐私锁只是应用访问门槛，不代表 SQLite 或备份文件已加密。

#### 外观、动态字体与辅助功能

1. 主题支持跟随系统、浅色和深色。启动健康检查完成前应用根节点即应用保存主题，避免先闪浅色再切换。
2. `reduce_motion` 为真或系统声明 `prefers-reduced-motion: reduce` 时，禁用非必要 transition、平滑滚动和装饰动画。
3. 字体使用相对单位并尊重 WebView 系统字体缩放；正文不设置不可缩放的固定像素字号，主要触控目标在放大字体后仍至少 44px。
4. 触感反馈默认开启，只在用户主动打勾等确认动作触发；不因保存失败、提醒权限拒绝或普通导航震动。不支持平台直接无副作用降级。
5. 所有开关包含可访问名称、说明和文本状态；焦点、错误和选中状态不得只靠颜色表达。

#### 设置页交互

1. 月历页提供明确的“设置”按钮。Android 系统返回从设置页回月历；锁屏时系统返回不绕过验证。
2. 开启提醒的顺序固定为：用户点击 → 解释本地提醒 → 请求权限 → 同步原生队列 → 成功后保存设置。任一步失败均不显示为已开启。
3. 时间、静默日、暂停一周和精确偏好修改后立即重新同步；忙碌期间禁用重复提交并显示低打扰状态。
4. “数据”区域复用 Phase 4 的受控备份能力，不在 Phase 5 开放前端通用文件系统权限。

#### 验收与测试

1. Rust 单元测试覆盖设置默认值、完整往返、非法时间/日期/主题/静默日和导入设置白名单。
2. 前端生产构建与 Tauri debug 构建通过；主题、减少动画、锁屏路由和设置失败恢复经过手动检查。
3. Android 真机覆盖通知允许/拒绝/撤销、默认不精确、精确授权降级、重启恢复、静默日、暂停一周、当日打勾取消和通知点击日期。
4. Android 真机覆盖生物识别成功、取消、失败、系统暂时锁定、后台 30 秒再次锁定和设备凭据回退；iOS 对等验证移入 v1 后续。
5. iOS 真机的未来 30 天队列、打勾后取消对应日期、启动补齐和权限撤销状态移入 v1 后续验证，不阻塞 v1。
6. 200% 动态字体、深浅主题、系统减少动画与横屏属于 v1 验收；小屏、TalkBack 完整人工听读和 VoiceOver 移入 v1 后续验证，不阻塞 v1。

#### PHASE 5 禁止提前实现

- 数据库或备份加密、自定义 PIN、云推送、远程配置、提醒统计、主题商店、复杂自动化规则。

#### PHASE 5 结束条件

实现上述最小闭环并完成可自动验证项目后停止，输出：`Phase 5 Local Experience Ready for Device Review`。真机矩阵未完成前不得称为商店发布就绪。

#### 当前实施状态（2026-07-14）

- Rust 设置层、设置页、主题、减少动画、触感开关和隐私锁状态机已实现。
- Android/iOS 本地提醒插件主体、桌面不支持降级和通知目标单次消费接口已实现。
- Android 原生工程和 arm64 debug APK 已真实编译，并在 vivo V2337A（Android 15 / API 35）完成基础月历、打勾、编辑保存、设置页和提醒冒烟验收。
- 大约时间提醒在允许 vivo 后台高耗电后可锁屏送达；精确提醒已确认为 `RTC_WAKEUP`、`window=0`，并在 22:00 真机准时触发。
- 通知目标已覆盖冷启动和后台 Activity 的 `onNewIntent` 路径；真实通知点击能回到对应民用日期，不自动打勾或打开编辑器。
- 通知权限撤销与恢复已完成真机闭环；前台恢复会协调通知和精确闹钟实际状态，并按现有设置自动取消或重排提醒。
- vivo 将 Gougou 的精确闹钟能力列入 allow-list，系统“闹钟与提醒”开关不可独立操作；已记录为设备限制，不通过关闭后台高耗电权限伪造授权降级。
- 隐私锁已覆盖开启、取消、成功、后台 31 秒重锁、内容遮挡和关闭时再次认证；连续失败、系统暂时锁定和设备凭据回退仍需持机人工验证。
- Android 图片选择、沙盒复制、受控协议显示和覆盖安装后的持久化已通过；真机发现的桌面协议硬编码已改为 Tauri 跨平台 URL 转换。删除引用后的安全回收仍待验证。
- 编辑器底栏已改为等宽弹性布局和 `clamp(0.75rem, 3.5vw, 0.875rem)` 流式字号，当前真机七个标签均完整横排并保留 44px 最小触控高度。
- 前端生产构建、15 个 Rust 测试、Android Gradle/Kotlin 编译、Rust 格式和差异空白检查已通过。
- 当前达到 `Phase 5 Local Experience Ready for Device Review` 并进入 Phase 6；Android 剩余图片安全回收、备份恢复、提醒边界、重启和辅助功能矩阵未完成，iOS 原生真机尚未开始，不能称为商店发布就绪。

### PHASE 6：真机与生命周期功能验收（当前执行阶段）

#### 范围与边界

1. 本阶段以验证和修复 Phase 1–5 的既有能力为主，不新增账号、云同步、统计 SDK、加密声明、性能重构或其他产品功能。
2. Android 为 v1 首发验收平台；iOS 对等验收、小屏设备和 TalkBack 完整人工听读记录为 v1 后续工作，不阻塞 v1。
3. 真机发现的问题采用最小修复，并补充能在本地自动执行的回归测试；不得为通过单一设备而放宽 IPC、文件路径或权限边界。
4. 未真实执行的设备、系统版本、权限场景和商店流程必须明确记录为未验证，不得用桌面构建结果替代。

#### 执行顺序

1. 配置 Java、Android SDK/NDK 与 Rust Android targets，完成 Android debug 编译；检查 Kotlin 插件 API、参数反序列化、Manifest 合并、PendingIntent 和通知图标。
2. Android 真机验证提醒权限允许/拒绝/撤销、不精确提醒、精确授权降级、静默日、暂停、当天打勾取消、重启恢复，以及冷启动和应用已运行时的通知目标导航。
3. Android 真机验证生物识别成功、取消、失败、暂时锁定、设备凭据回退、冷启动锁屏和后台超过 30 秒重锁。
4. 验证前台恢复时的提醒权限与精确闹钟状态刷新，并覆盖图片选择、自动保存、系统返回、备份导入导出等跨阶段回归；完整备份保留在 v1，必须从设置页提供正式生产入口，不能仅依赖 `import.meta.env.DEV` 下的验收页面。
5. 完成 200% 动态字体、系统减少动画、深浅主题、横屏和安全区检查；小屏与 TalkBack 完整人工听读移入 v1 后续。
6. iOS 原生编译、未来 30 条通知队列、权限撤销、通知点击、LocalAuthentication、VoiceOver、安全区和生命周期恢复移入 v1 后续。
7. 验证首次安装、现有数据库启动、备份恢复失败回滚、后台恢复和现有数据兼容；性能、包体、Release 构建与发布工程集中进入 Phase 7。

#### 验收记录

每个真机场景记录：平台与系统版本、设备或模拟器、操作步骤、预期结果、实际结果、日志或截图位置和结论。需要人工操作时提供逐步说明，不要求用户自行推断设置入口或成功标准。

#### 当前 Phase 6 检查点（2026-07-21）

1. 已完成 Android debug 工程、arm64 APK、基础月历/编辑保存、通知送达与目标日期导航、通知权限撤销恢复、静默日/暂停/当天打勾重排、隐私锁自动安全路径、图片选择/复制/显示/删除/零引用回收/重启持久化、备份导出/篡改拒绝/有效恢复和编辑器底栏真机回归。
2. 主题三态和应用内减少动画已通过；当前设备/WebView 不映射系统减少动画媒体查询，已明确记录为平台限制。200% + IME 组合、横屏月历/编辑器/设置页和完整 Tab 顺序均已通过，字体与旋转已恢复原值。
3. 真机发现 WebView 126 不会在 IME 显示时更新 `visualViewport`，导致编辑器工具栏被键盘覆盖；已用 Android `WindowInsetsCompat.Type.ime()` 向前端提供真实 inset，并与现有 `visualViewport` 偏移取较大值。字体 1.0 与 2.0 下均完成软键盘回归。
4. 编辑器 sticky 顶栏、设置页和备份验收页已统一使用至少 1.5rem 的顶部安全区留白。编辑器与设置页完成覆盖安装和真机回归；备份验收页仅开发构建可达，当前生产前端资源不能单独截图。
5. TalkBack 服务、触摸探索与 Gougou 语义树自动化路径通过并恢复原值；完整人工听读、小屏设备和 iOS 真机已记录为 v1 后续工作，不再作为 v1 阻塞项。
6. vivo 自启动关闭时系统不投递 Gougou 的 `BOOT_COMPLETED`。设置页已增加用户主动打开自启动管理页的入口，并保留应用冷启动自愈；临时允许自启动后的真实重启中，系统启动 `ReminderBootReceiver` 并在 Activity 未启动时恢复 22:02 精确闹钟，数据库哈希不变；验证结束后自启动已恢复为原始关闭状态。
7. 完整备份核心、事务、篡改拒绝和有效恢复已经验收；现有 `BackupTestView` 仍是开发模式内部控制台。备份确定保留在 v1，下一项工作是在设置页“数据”区域提供正式导出、导入预检、合并较新内容和二次确认替换入口，并复用既有受控 IPC 与系统文件对话框。

#### PHASE 6 结束条件

1. Android v1 首发矩阵全部通过；已明确移入 v1 后续的小屏、TalkBack 完整人工听读和 iOS 对等能力不计入本条件。
2. 自动测试、生产前端构建、Android 原生 debug 编译与数据库/备份回归通过，且 v1 完整备份可从生产设置页进入。
3. 真机发现的功能和生命周期缺陷均已做最小修复并完成对应回归；未完成项有明确风险和阻塞原因。
4. 达到上述条件后输出：`Phase 6 Device Matrix Ready for Optimization`，随后进入 Phase 7；不得提前称为 Release Candidate。

### PHASE 7：性能、内存、包体与发布工程（计划阶段）

#### 背景与原则

1. 当前最新 arm64 debug APK 实测约 383 MB，用户同时观察到运行时“内存占用 300 MB 以上”。这两个数字都需要优化，但含义不同：APK 体积是磁盘/分发指标，运行时内存必须进一步拆分为 PSS、RSS、USS、WebView renderer 和系统缓存。
2. debug APK 包含调试符号和未按发布条件优化的原生库，不能直接推断 release APK/AAB 体积；Android 系统设置显示的内存数字也不能在没有采样口径时直接判定泄漏。
3. 先测量、再定位、后优化。每次只改变一个主要因素并记录前后数据，不以删除功能、降低图片安全校验、缩短备份保留或放宽隐私边界换取数字。
4. 优先使用构建配置、ABI 拆分、资源裁剪、懒加载和生命周期释放等局部手段；没有数据证明前不得重写 Tauri/React/Tiptap 架构。

#### 必须建立的基线

1. 在同一台 vivo V2337A 上分别测量 debug 与 release：
   - arm64 APK 文件大小、AAB 大小和 bundletool 估算下载大小；
   - 安装后的应用数据、代码与缓存占用；
   - 冷启动 `am start -W` 的 TotalTime/WaitTime；
   - 冷启动后静置 60 秒的应用主进程、WebView renderer 和合计 PSS/RSS/USS。
2. 运行时至少覆盖：月历静置、空编辑器、长文编辑器、含多图日记、设置页、进入后台 60 秒和返回前台。
3. 每个稳定场景至少采样 3 次并记录中位数；采样前明确是否触发 GC、是否联网、屏幕方向、主题和字体缩放，禁止挑选单次最低值。
4. 做 20 次“月历 ↔ 编辑器”和 20 次“前台 ↔ 后台”循环，静置后再次采样；持续增长且不能回落的内存才进入泄漏排查。
5. 生成包体组成报告，至少列出 Rust `.so`、WebView/Android 依赖、中文字体、前端 JS/CSS、图片资源和签名元数据的占比。

#### 优化顺序

1. 先构建干净的 arm64 release APK/AAB，启用适用的符号剥离、release Rust profile 和 Android 资源压缩；确认产物不混入其他 ABI、debug symbols、开发验收入口或本地绝对路径。
2. 检查 Tauri/Rust 原生库大小，评估 `strip`、LTO、`codegen-units`、`panic=abort` 等 release 选项；每项必须通过数据库、提醒、图片和备份回归后保留。
3. 检查 Noto Sans SC 字体资产是否重复或过度打包，在不破坏离线中文、动态字体和字形覆盖的前提下做子集化或按需加载。
4. 对前端 bundle 做可视化分析；优先延迟加载只在编辑页使用的 Tiptap/Markdown/图片模块，月历首屏不得预加载不需要的编辑器实例。
5. 检查编辑器销毁、图片 object URL、`visualViewport`/生命周期监听器、Tauri event listener 和大字符串草稿是否在离开页面后释放；只修复有快照或重复采样证据的保留链。
6. 多图场景只加载受控缩略图并使用懒加载，禁止为降低内存而绕过原图尺寸限制、引用完整性或沙盒协议。
7. 完成优化后重新执行 Phase 1–6 的关键回归，再进行 release 签名、权限、Manifest、商店政策和安装升级检查。

#### 暂定性能门槛

以下门槛在取得第一轮 release 基线后允许收紧；若受 Tauri/WebView 平台下限影响无法达到，必须给出可复现数据和组成解释，不能静默放宽：

1. arm64 release APK 不高于 50 MiB，AAB 的 arm64 估算下载大小不高于 35 MiB；任何单项超过产物 25% 都必须列入组成报告。
2. vivo 真机月历静置 60 秒后的合计 PSS 目标不高于 160 MiB；若初始 release 基线高于 200 MiB，至少降低 25%，并证明剩余部分主要来自必要的系统 WebView 或原生运行时。
3. 20 次页面和前后台循环后，静置 PSS 相对首次静置增长不超过 20 MiB，且不存在随循环次数单调增长的未释放资源。
4. 当前 vivo 真机 release 冷启动 TotalTime 目标不高于 800ms；优化不得让编辑保存、图片加载或通知启动产生可感知回退。
5. 所有门槛同时要求功能回归、无障碍和本地隐私边界不退化；只改善某一数字但破坏功能视为失败。

#### 发布检查

1. 复核 `SCHEDULE_EXACT_ALARM` 的 Google Play 政策资格，并决定是否保留“尽量准时”。
2. 检查 release Manifest 是否移除模板默认且业务不需要的 `INTERNET` 权限。
3. 验证 Android release APK/AAB 的 ABI、签名、最小/目标 SDK、权限、升级安装和备份恢复；iOS 对应检查 archive、entitlements、隐私清单和签名。
4. 构建产物、性能原始数据和分析报告可归档到明确的验收目录，但不得提交密钥、签名密码、设备私密数据或本机绝对路径缓存。

#### PHASE 7 结束条件

1. 包体、内存、启动时间和循环稳定性均有可复现的优化前后数据，并达到门槛或存在经审核接受的书面例外。
2. Android/iOS release 构建、签名权限、升级安装、数据库/备份和 Phase 6 关键真机回归通过。
3. 商店政策、隐私声明、精确闹钟资格和最终权限清单经过人工复核。
4. 达到上述条件后才可输出：`Phase 7 Release Candidate Ready for Review`。
