use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Duration, Local};
use pulldown_cmark::{Event, Parser, Tag};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_gougou_reminder::{
    GougouReminderExt, NotificationTarget, ReminderSchedule, ReminderStatus,
};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const SCHEMA_VERSION: i64 = 1;
const BACKUP_FORMAT_VERSION: u8 = 1;
const IMPORT_TOKEN_TTL_SECONDS: i64 = 15 * 60;
const MAX_BACKUP_ENTRIES: usize = 10_000;
const MAX_BACKUP_JSON_BYTES: u64 = 20 * 1024 * 1024;
const MAX_BACKUP_ASSET_BYTES: u64 = 10 * 1024 * 1024;
const MAX_BACKUP_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const SETTING_KEYS: [&str; 9] = [
    "reminder.enabled",
    "reminder.hour",
    "reminder.minute",
    "reminder.precise",
    "reminder.quiet_weekdays",
    "reminder.paused_until",
    "privacy.lock_enabled",
    "appearance.theme",
    "accessibility.reduce_motion",
];
const HAPTICS_SETTING_KEY: &str = "accessibility.haptics";

struct Database(Mutex<Connection>);
struct PendingImports(Mutex<HashMap<String, PendingImport>>);

#[derive(Debug)]
struct PendingImport {
    staging: PathBuf,
    expires_at: i64,
    manifest: BackupManifest,
    data: BackupData,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonthEntrySummary {
    entry_date: String,
    is_ticked: bool,
    has_content: bool,
    updated_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryDetail {
    exists: bool,
    entry_date: String,
    content_md: String,
    word_count: i64,
    revision: i64,
    is_ticked: bool,
    updated_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthStatus {
    database_ready: bool,
    schema_version: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupExport {
    file_name: String,
    source_path: String,
    entry_count: usize,
    asset_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupPreview {
    import_token: String,
    entry_count: usize,
    asset_count: usize,
    conflict_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupImport {
    imported_entries: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupEntry {
    id: String,
    entry_date: String,
    is_ticked: bool,
    content_md: String,
    word_count: i64,
    revision: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupAssetReference {
    entry_id: String,
    asset_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupSetting {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupData {
    entries: Vec<BackupEntry>,
    entry_assets: Vec<BackupAssetReference>,
    user_settings: Vec<BackupSetting>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupAssetManifest {
    name: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupManifest {
    format_version: u8,
    exported_at: i64,
    assets: Vec<BackupAssetManifest>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetDetail {
    asset_name: String,
    preview_url: String,
    mime_type: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ReminderSettings {
    enabled: bool,
    hour: u8,
    minute: u8,
    precise: bool,
    quiet_weekdays: Vec<u8>,
    paused_until: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PrivacySettings {
    lock_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AppearanceSettings {
    theme: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AccessibilitySettings {
    reduce_motion: bool,
    haptics: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    reminder: ReminderSettings,
    privacy: PrivacySettings,
    appearance: AppearanceSettings,
    accessibility: AccessibilitySettings,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BiometricPlatformStatus {
    supported: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            reminder: ReminderSettings {
                enabled: false,
                hour: 22,
                minute: 0,
                precise: false,
                quiet_weekdays: Vec::new(),
                paused_until: None,
            },
            privacy: PrivacySettings {
                lock_enabled: false,
            },
            appearance: AppearanceSettings {
                theme: "system".into(),
            },
            accessibility: AccessibilitySettings {
                reduce_motion: false,
                haptics: true,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct CommandError {
    code: &'static str,
    message: &'static str,
}

type CommandResult<T> = Result<T, CommandError>;

impl CommandError {
    const fn invalid_date() -> Self {
        Self {
            code: "invalid_date",
            message: "日期格式无效。",
        }
    }

    const fn database() -> Self {
        Self {
            code: "database_error",
            message: "本地数据暂时无法读取，请稍后重试。",
        }
    }

    const fn invalid_revision() -> Self {
        Self {
            code: "invalid_revision",
            message: "保存版本无效。",
        }
    }

    const fn content_too_large() -> Self {
        Self {
            code: "content_too_large",
            message: "这篇记录太长了，请分成几次写。",
        }
    }

    const fn revision_conflict() -> Self {
        Self {
            code: "revision_conflict",
            message: "这篇记录已有更新，请重新打开后再试。",
        }
    }

    const fn invalid_asset() -> Self {
        Self {
            code: "invalid_asset",
            message: "图片引用无效。",
        }
    }

    const fn invalid_backup() -> Self {
        Self {
            code: "invalid_backup",
            message: "备份文件无效或已损坏。",
        }
    }

    const fn invalid_import_token() -> Self {
        Self {
            code: "invalid_import_token",
            message: "导入确认已失效，请重新选择备份。",
        }
    }

    const fn import_token_expired() -> Self {
        Self {
            code: "import_token_expired",
            message: "导入确认已过期，请重新选择备份。",
        }
    }

    const fn invalid_import_mode() -> Self {
        Self {
            code: "invalid_import_mode",
            message: "导入方式无效。",
        }
    }

    const fn invalid_settings() -> Self {
        Self {
            code: "invalid_settings",
            message: "设置内容无效。",
        }
    }

    const fn reminder_unavailable() -> Self {
        Self {
            code: "reminder_unavailable",
            message: "当前设备暂不支持本地提醒。",
        }
    }

    const fn reminder_failed() -> Self {
        Self {
            code: "reminder_failed",
            message: "提醒暂时没有设置成功。",
        }
    }

    #[cfg(any(mobile, not(debug_assertions)))]
    const fn unsupported_platform() -> Self {
        Self {
            code: "unsupported_platform",
            message: "当前平台暂不支持这个操作。",
        }
    }
}

fn app_database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法定位应用数据目录".to_owned())?;
    let database_dir = app_data_dir.join("db");
    fs::create_dir_all(&database_dir).map_err(|_| "无法创建应用数据目录".to_owned())?;
    Ok(database_dir.join("gougou.db"))
}

fn backup_directory(app: &AppHandle) -> Result<PathBuf, CommandError> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::database())?
        .join("tmp")
        .join("exports");
    fs::create_dir_all(&directory).map_err(|_| CommandError::database())?;
    Ok(directory)
}

fn import_staging_directory(app: &AppHandle, token: &str) -> Result<PathBuf, CommandError> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::database())?
        .join("tmp")
        .join(format!("import-{token}"));
    fs::create_dir_all(&directory).map_err(|_| CommandError::database())?;
    Ok(directory)
}

fn cleanup_expired_temporary_files(app: &AppHandle) {
    let Ok(directory) = app.path().app_data_dir().map(|path| path.join("tmp")) else {
        return;
    };
    let Ok(files) = fs::read_dir(directory) else {
        return;
    };
    for file in files.flatten() {
        let Ok(modified) = file.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if SystemTime::now()
            .duration_since(modified)
            .is_ok_and(|age| age.as_secs() > 24 * 60 * 60)
        {
            if file.file_type().is_ok_and(|kind| kind.is_dir()) {
                let _ = fs::remove_dir_all(file.path());
            } else {
                let _ = fs::remove_file(file.path());
            }
        }
    }
}

fn backup_snapshot(connection: &Connection) -> CommandResult<BackupData> {
    let mut entries_statement = connection
        .prepare("SELECT id, entry_date, is_ticked, content_md, word_count, revision, created_at, updated_at FROM entries ORDER BY entry_date")
        .map_err(|_| CommandError::database())?;
    let entries = entries_statement
        .query_map([], |row| {
            Ok(BackupEntry {
                id: row.get(0)?,
                entry_date: row.get(1)?,
                is_ticked: row.get::<_, i64>(2)? != 0,
                content_md: row.get(3)?,
                word_count: row.get(4)?,
                revision: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|_| CommandError::database())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CommandError::database())?;
    let mut assets_statement = connection
        .prepare("SELECT entry_id, asset_name FROM entry_assets ORDER BY entry_id, asset_name")
        .map_err(|_| CommandError::database())?;
    let entry_assets = assets_statement
        .query_map([], |row| {
            Ok(BackupAssetReference {
                entry_id: row.get(0)?,
                asset_name: row.get(1)?,
            })
        })
        .map_err(|_| CommandError::database())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CommandError::database())?;
    let mut settings_statement = connection
        .prepare("SELECT key, value FROM user_settings ORDER BY key")
        .map_err(|_| CommandError::database())?;
    let user_settings = settings_statement
        .query_map([], |row| {
            Ok(BackupSetting {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(|_| CommandError::database())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CommandError::database())?;
    Ok(BackupData {
        entries,
        entry_assets,
        user_settings,
    })
}

fn migrate(connection: &mut Connection) -> Result<(), String> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| "无法读取数据库版本".to_owned())?;

    if version > SCHEMA_VERSION {
        return Err("数据库版本不受当前应用支持".to_owned());
    }

    if version == 0 {
        let transaction = connection
            .transaction()
            .map_err(|_| "无法开始数据库迁移".to_owned())?;
        transaction
            .execute_batch(
                "
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
                PRAGMA user_version = 1;
                ",
            )
            .map_err(|_| "无法执行数据库迁移".to_owned())?;
        transaction
            .commit()
            .map_err(|_| "无法完成数据库迁移".to_owned())?;
    }

    Ok(())
}

fn open_database(app: &AppHandle) -> Result<Connection, String> {
    let mut connection =
        Connection::open(app_database_path(app)?).map_err(|_| "无法打开本地数据库".to_owned())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "无法配置数据库".to_owned())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|_| "无法配置数据库".to_owned())?;
    migrate(&mut connection)?;
    Ok(connection)
}

fn database_health(connection: &Connection) -> CommandResult<HealthStatus> {
    let schema_version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|_| CommandError::database())?;
    let table_count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name IN ('entries', 'entry_assets', 'user_settings')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| CommandError::database())?;
    if schema_version != SCHEMA_VERSION || table_count != 3 {
        return Err(CommandError::database());
    }
    Ok(HealthStatus {
        database_ready: true,
        schema_version,
    })
}

#[tauri::command]
fn health_check(database: State<'_, Database>) -> CommandResult<HealthStatus> {
    let connection = database.0.lock().map_err(|_| CommandError::database())?;
    database_health(&connection)
}

fn valid_setting_key(key: &str) -> bool {
    SETTING_KEYS.contains(&key) || key == HAPTICS_SETTING_KEY
}

fn validate_app_settings(settings: &AppSettings) -> CommandResult<()> {
    let reminder = &settings.reminder;
    let mut weekdays = BTreeSet::new();
    if reminder.hour > 23
        || reminder.minute > 59
        || reminder
            .quiet_weekdays
            .iter()
            .any(|day| !(1..=7).contains(day) || !weekdays.insert(*day))
        || reminder
            .paused_until
            .as_deref()
            .is_some_and(|date| !valid_date(date))
        || !matches!(
            settings.appearance.theme.as_str(),
            "system" | "light" | "dark"
        )
    {
        return Err(CommandError::invalid_settings());
    }
    Ok(())
}

fn read_app_settings(connection: &Connection) -> CommandResult<AppSettings> {
    let mut statement = connection
        .prepare("SELECT key, value FROM user_settings")
        .map_err(|_| CommandError::database())?;
    let values = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| CommandError::database())?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|_| CommandError::database())?;
    let parse_bool = |key: &str, default: bool| -> CommandResult<bool> {
        match values.get(key).map(String::as_str) {
            None => Ok(default),
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            Some(_) => Err(CommandError::database()),
        }
    };
    let parse_u8 = |key: &str, default: u8| -> CommandResult<u8> {
        values.get(key).map_or(Ok(default), |value| {
            value.parse().map_err(|_| CommandError::database())
        })
    };
    let quiet_weekdays = values
        .get("reminder.quiet_weekdays")
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .split(',')
                .map(|day| day.parse::<u8>().map_err(|_| CommandError::database()))
                .collect::<CommandResult<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let settings = AppSettings {
        reminder: ReminderSettings {
            enabled: parse_bool("reminder.enabled", false)?,
            hour: parse_u8("reminder.hour", 22)?,
            minute: parse_u8("reminder.minute", 0)?,
            precise: parse_bool("reminder.precise", false)?,
            quiet_weekdays,
            paused_until: values
                .get("reminder.paused_until")
                .filter(|value| !value.is_empty())
                .cloned(),
        },
        privacy: PrivacySettings {
            lock_enabled: parse_bool("privacy.lock_enabled", false)?,
        },
        appearance: AppearanceSettings {
            theme: values
                .get("appearance.theme")
                .cloned()
                .unwrap_or_else(|| "system".into()),
        },
        accessibility: AccessibilitySettings {
            reduce_motion: parse_bool("accessibility.reduce_motion", false)?,
            haptics: parse_bool(HAPTICS_SETTING_KEY, true)?,
        },
    };
    validate_app_settings(&settings)?;
    Ok(settings)
}

fn write_app_settings(connection: &mut Connection, settings: &AppSettings) -> CommandResult<()> {
    validate_app_settings(settings)?;
    let transaction = connection
        .transaction()
        .map_err(|_| CommandError::database())?;
    let quiet_weekdays = settings
        .reminder
        .quiet_weekdays
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let values = [
        ("reminder.enabled", settings.reminder.enabled.to_string()),
        ("reminder.hour", settings.reminder.hour.to_string()),
        ("reminder.minute", settings.reminder.minute.to_string()),
        ("reminder.precise", settings.reminder.precise.to_string()),
        ("reminder.quiet_weekdays", quiet_weekdays),
        (
            "reminder.paused_until",
            settings.reminder.paused_until.clone().unwrap_or_default(),
        ),
        (
            "privacy.lock_enabled",
            settings.privacy.lock_enabled.to_string(),
        ),
        ("appearance.theme", settings.appearance.theme.clone()),
        (
            "accessibility.reduce_motion",
            settings.accessibility.reduce_motion.to_string(),
        ),
        (
            HAPTICS_SETTING_KEY,
            settings.accessibility.haptics.to_string(),
        ),
    ];
    for (key, value) in values {
        transaction
            .execute(
                "INSERT INTO user_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|_| CommandError::database())?;
    }
    transaction.commit().map_err(|_| CommandError::database())
}

#[tauri::command]
fn get_app_settings(database: State<'_, Database>) -> CommandResult<AppSettings> {
    let connection = database.0.lock().map_err(|_| CommandError::database())?;
    read_app_settings(&connection)
}

#[tauri::command]
fn get_biometric_platform_status() -> BiometricPlatformStatus {
    BiometricPlatformStatus {
        supported: cfg!(mobile),
    }
}

#[tauri::command]
fn update_app_settings(
    settings: AppSettings,
    database: State<'_, Database>,
) -> CommandResult<AppSettings> {
    let mut connection = database.0.lock().map_err(|_| CommandError::database())?;
    write_app_settings(&mut connection, &settings)?;
    Ok(settings)
}

fn reminder_schedule(
    connection: &Connection,
    settings: ReminderSettings,
) -> CommandResult<ReminderSchedule> {
    let today = Local::now().date_naive();
    let last_date = today + Duration::days(45);
    let mut statement = connection
        .prepare(
            "SELECT entry_date FROM entries
             WHERE is_ticked = 1 AND entry_date >= ?1 AND entry_date <= ?2
             ORDER BY entry_date",
        )
        .map_err(|_| CommandError::database())?;
    let skip_dates = statement
        .query_map(
            params![
                today.format("%Y-%m-%d").to_string(),
                last_date.format("%Y-%m-%d").to_string()
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| CommandError::database())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CommandError::database())?;
    Ok(ReminderSchedule {
        enabled: settings.enabled,
        hour: settings.hour,
        minute: settings.minute,
        precise: settings.precise,
        quiet_weekdays: settings.quiet_weekdays,
        paused_until: settings.paused_until,
        skip_dates,
        title: "勾勾".into(),
        body: "今天还没有留下一个勾勾。要不要和今天打个招呼？".into(),
    })
}

#[cfg_attr(not(mobile), allow(dead_code))]
fn sync_reminder_from_database(
    app: &AppHandle,
    connection: &Connection,
) -> CommandResult<ReminderStatus> {
    let settings = read_app_settings(connection)?;
    let schedule = reminder_schedule(connection, settings.reminder)?;
    app.gougou_reminder()
        .sync_schedule(schedule)
        .map_err(|_| CommandError::reminder_failed())
}

#[tauri::command]
fn get_reminder_status(app: AppHandle) -> CommandResult<ReminderStatus> {
    app.gougou_reminder()
        .get_status()
        .map_err(|_| CommandError::reminder_failed())
}

#[tauri::command]
fn take_reminder_target(app: AppHandle) -> CommandResult<NotificationTarget> {
    let mut target = app
        .gougou_reminder()
        .take_notification_target()
        .map_err(|_| CommandError::reminder_failed())?;
    if target
        .target_date
        .as_deref()
        .is_some_and(|date| !valid_date(date))
    {
        target.target_date = None;
    }
    Ok(target)
}

#[tauri::command]
fn request_reminder_permission(app: AppHandle) -> CommandResult<ReminderStatus> {
    let status = app
        .gougou_reminder()
        .request_permission()
        .map_err(|_| CommandError::reminder_failed())?;
    if !status.supported {
        return Err(CommandError::reminder_unavailable());
    }
    Ok(status)
}

#[tauri::command]
fn sync_reminder(
    app: AppHandle,
    reminder: ReminderSettings,
    database: State<'_, Database>,
) -> CommandResult<ReminderStatus> {
    let proposed = AppSettings {
        reminder: reminder.clone(),
        ..AppSettings::default()
    };
    validate_app_settings(&proposed)?;
    let connection = database.0.lock().map_err(|_| CommandError::database())?;
    let schedule = reminder_schedule(&connection, reminder)?;
    let status = app
        .gougou_reminder()
        .sync_schedule(schedule)
        .map_err(|_| CommandError::reminder_failed())?;
    if !status.supported {
        return Err(CommandError::reminder_unavailable());
    }
    Ok(status)
}

fn days_in_month(year: u32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn parse_number(value: &str) -> Option<u32> {
    value.parse().ok()
}

fn valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }

    let year = parse_number(&date[0..4]);
    let month = parse_number(&date[5..7]);
    let day = parse_number(&date[8..10]);

    matches!((year, month, day), (Some(year), Some(month), Some(day)) if days_in_month(year, month).is_some_and(|max_day| day > 0 && day <= max_day))
}

fn month_range(month: &str) -> Option<(String, String)> {
    let bytes = month.as_bytes();
    if bytes.len() != 7
        || bytes[4] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || byte.is_ascii_digit())
    {
        return None;
    }

    let year = parse_number(&month[0..4])?;
    let month_number = parse_number(&month[5..7])?;
    if !(1..=12).contains(&month_number) {
        return None;
    }

    let (next_year, next_month) = if month_number == 12 {
        (year + 1, 1)
    } else {
        (year, month_number + 1)
    };
    Some((month.to_owned(), format!("{next_year:04}-{next_month:02}")))
}

fn now_unix_seconds() -> Result<i64, CommandError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .map_err(|_| CommandError::database())
}

fn empty_entry_detail(date: String) -> EntryDetail {
    EntryDetail {
        exists: false,
        entry_date: date,
        content_md: String::new(),
        word_count: 0,
        revision: 0,
        is_ticked: false,
        updated_at: 0,
    }
}

fn is_cjk_character(character: char) -> bool {
    matches!(
        character,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
    )
}

fn word_count(markdown: &str) -> i64 {
    let mut count = 0;
    let mut in_latin_word = false;

    for event in Parser::new(markdown) {
        let text = match event {
            Event::Text(text) | Event::Code(text) => text,
            Event::SoftBreak | Event::HardBreak => {
                in_latin_word = false;
                continue;
            }
            _ => continue,
        };

        for character in text.chars() {
            if is_cjk_character(character) {
                count += 1;
                in_latin_word = false;
            } else if character.is_ascii_alphanumeric() {
                if !in_latin_word {
                    count += 1;
                    in_latin_word = true;
                }
            } else {
                in_latin_word = false;
            }
        }
    }

    count
}

fn markdown_asset_references(markdown: &str) -> CommandResult<BTreeSet<String>> {
    let mut references = BTreeSet::new();
    for event in Parser::new(markdown) {
        let Event::Start(Tag::Image { dest_url, .. }) = event else {
            continue;
        };
        let source = dest_url.as_ref();
        let Some(name) = source.strip_prefix("assets/") else {
            return Err(CommandError::invalid_asset());
        };
        if !valid_original_asset_file_name(name) {
            return Err(CommandError::invalid_asset());
        }
        references.insert(name.to_owned());
    }
    Ok(references)
}

fn valid_asset_file_name(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    let uuid = stem.strip_suffix(".thumb").unwrap_or(stem);
    matches!(extension, "png" | "jpg" | "jpeg" | "webp")
        && Uuid::parse_str(uuid).is_ok_and(|value| value.to_string() == uuid)
}

fn valid_original_asset_file_name(name: &str) -> bool {
    valid_asset_file_name(name) && !name.contains(".thumb.")
}

fn asset_content_type(name: &str) -> &'static str {
    if name.ends_with(".png") {
        "image/png"
    } else if name.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

fn import_image_from_path(app: &AppHandle, source: PathBuf) -> CommandResult<AssetDetail> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::invalid_asset())?;
    let metadata = fs::metadata(&source).map_err(|_| CommandError::invalid_asset())?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err(CommandError::invalid_asset());
    }
    let detected = infer::get_from_path(&source)
        .map_err(|_| CommandError::invalid_asset())?
        .ok_or_else(CommandError::invalid_asset)?;
    let extension = match detected.mime_type() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => return Err(CommandError::invalid_asset()),
    };
    if source
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| {
            !value.eq_ignore_ascii_case(extension)
                && !(extension == "jpg" && value.eq_ignore_ascii_case("jpeg"))
        })
    {
        return Err(CommandError::invalid_asset());
    }
    let (width, height) =
        image::image_dimensions(&source).map_err(|_| CommandError::invalid_asset())?;
    if width > 4096 || height > 4096 || u64::from(width) * u64::from(height) > 20_000_000 {
        return Err(CommandError::invalid_asset());
    }
    let assets_dir = app_data.join("assets");
    fs::create_dir_all(&assets_dir).map_err(|_| CommandError::database())?;
    let stem = Uuid::new_v4().to_string();
    let asset_name = format!("{stem}.{extension}");
    let asset_path = assets_dir.join(&asset_name);
    let thumbnail_path = assets_dir.join(format!("{stem}.thumb.webp"));
    let thumbnail = image::open(&source)
        .map_err(|_| CommandError::invalid_asset())?
        .thumbnail(512, 512);
    fs::copy(&source, &asset_path).map_err(|_| CommandError::database())?;
    if thumbnail
        .save_with_format(&thumbnail_path, image::ImageFormat::WebP)
        .is_err()
    {
        let _ = fs::remove_file(&asset_path);
        return Err(CommandError::database());
    }
    Ok(AssetDetail {
        preview_url: format!("gougou-asset://localhost/{asset_name}"),
        asset_name: format!("assets/{asset_name}"),
        mime_type: detected.mime_type().to_owned(),
        width,
        height,
    })
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn copy_android_content_uri(
    app: &AppHandle,
    source: tauri_plugin_fs::FilePath,
) -> CommandResult<PathBuf> {
    use tauri_plugin_fs::{FsExt, OpenOptions};

    let tauri_plugin_fs::FilePath::Url(url) = &source else {
        return Err(CommandError::invalid_asset());
    };
    if url.scheme() != "content" {
        return Err(CommandError::invalid_asset());
    }

    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::invalid_asset())?
        .join("tmp")
        .join("picked-images");
    fs::create_dir_all(&directory).map_err(|_| CommandError::database())?;
    let pending = directory.join(format!("{}.pending", Uuid::new_v4()));

    let result = (|| {
        let mut options = OpenOptions::new();
        options.read(true);
        let input = app
            .fs()
            .open(source, options.clone())
            .map_err(|_| CommandError::invalid_asset())?;
        let mut output = fs::File::create(&pending).map_err(|_| CommandError::database())?;
        let mut limited = input.take(MAX_IMAGE_BYTES + 1);
        let copied =
            std::io::copy(&mut limited, &mut output).map_err(|_| CommandError::invalid_asset())?;
        output.flush().map_err(|_| CommandError::database())?;
        if copied > MAX_IMAGE_BYTES {
            return Err(CommandError::invalid_asset());
        }

        let detected = infer::get_from_path(&pending)
            .map_err(|_| CommandError::invalid_asset())?
            .ok_or_else(CommandError::invalid_asset)?;
        let extension = match detected.mime_type() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => return Err(CommandError::invalid_asset()),
        };
        let copied = pending.with_extension(extension);
        fs::rename(&pending, &copied).map_err(|_| CommandError::database())?;
        Ok(copied)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

#[tauri::command]
fn import_image(app: AppHandle, source_path: String) -> CommandResult<AssetDetail> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::invalid_asset())?;
    let temporary_root =
        fs::canonicalize(app_data.join("tmp")).map_err(|_| CommandError::invalid_asset())?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .ok()
        .and_then(|path| fs::canonicalize(path).ok());
    let source = fs::canonicalize(source_path).map_err(|_| CommandError::invalid_asset())?;
    if !source.starts_with(&temporary_root)
        && !cache_root
            .as_ref()
            .is_some_and(|root| source.starts_with(root))
    {
        return Err(CommandError::invalid_asset());
    }
    import_image_from_path(&app, source)
}

#[tauri::command]
fn pick_and_import_image(app: AppHandle) -> CommandResult<Option<AssetDetail>> {
    #[cfg(desktop)]
    {
        use tauri_plugin_dialog::DialogExt;

        let selected = app
            .dialog()
            .file()
            .add_filter("图片", &["png", "jpg", "jpeg", "webp"])
            .blocking_pick_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let source = selected
            .into_path()
            .map_err(|_| CommandError::invalid_asset())?;
        let source = fs::canonicalize(source).map_err(|_| CommandError::invalid_asset())?;
        return import_image_from_path(&app, source).map(Some);
    }

    #[cfg(target_os = "android")]
    {
        use tauri_plugin_dialog::{DialogExt, PickerMode};

        let selected = app
            .dialog()
            .file()
            .set_picker_mode(PickerMode::Image)
            .add_filter("图片", &["image/png", "image/jpeg", "image/webp"])
            .blocking_pick_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let source = copy_android_content_uri(&app, selected)?;
        let imported = import_image_from_path(&app, source.clone());
        let _ = fs::remove_file(source);
        imported.map(Some)
    }

    #[cfg(target_os = "ios")]
    Err(CommandError::unsupported_platform())
}

fn replace_entry_assets(
    transaction: &rusqlite::Transaction<'_>,
    entry_id: &str,
    references: &BTreeSet<String>,
) -> CommandResult<()> {
    transaction
        .execute(
            "DELETE FROM entry_assets WHERE entry_id = ?1",
            params![entry_id],
        )
        .map_err(|_| CommandError::database())?;
    for asset_name in references {
        transaction
            .execute(
                "INSERT INTO entry_assets (entry_id, asset_name) VALUES (?1, ?2)",
                params![entry_id, asset_name],
            )
            .map_err(|_| CommandError::database())?;
    }
    Ok(())
}

fn referenced_assets(connection: &Connection) -> CommandResult<BTreeSet<String>> {
    let mut statement = connection
        .prepare("SELECT DISTINCT asset_name FROM entry_assets")
        .map_err(|_| CommandError::database())?;
    let referenced = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| CommandError::database())?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|_| CommandError::database())?;
    Ok(referenced)
}

fn cleanup_unreferenced_assets(
    app: &AppHandle,
    referenced: &BTreeSet<String>,
) -> CommandResult<()> {
    let assets_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::database())?
        .join("assets");
    let files = match fs::read_dir(&assets_dir) {
        Ok(files) => files,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(CommandError::database()),
    };
    for file in files.flatten() {
        let name = file.file_name().to_string_lossy().to_string();
        let thumbnail_is_referenced = name.strip_suffix(".thumb.webp").is_some_and(|stem| {
            referenced.iter().any(|original| {
                original
                    .strip_suffix(".png")
                    .or_else(|| original.strip_suffix(".jpg"))
                    .or_else(|| original.strip_suffix(".jpeg"))
                    .or_else(|| original.strip_suffix(".webp"))
                    == Some(stem)
            })
        });
        if valid_asset_file_name(&name) && !referenced.contains(&name) && !thumbnail_is_referenced {
            let _ = fs::remove_file(file.path());
        }
    }
    Ok(())
}

fn read_entry_detail(connection: &Connection, date: String) -> CommandResult<EntryDetail> {
    let requested_date = date.clone();
    let detail = connection
        .query_row(
            "SELECT entry_date, content_md, word_count, revision, is_ticked, updated_at
             FROM entries WHERE entry_date = ?1",
            params![date],
            |row| {
                Ok(EntryDetail {
                    exists: true,
                    entry_date: row.get(0)?,
                    content_md: row.get(1)?,
                    word_count: row.get(2)?,
                    revision: row.get(3)?,
                    is_ticked: row.get::<_, i64>(4)? != 0,
                    updated_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|_| CommandError::database())?;
    Ok(detail.unwrap_or_else(|| empty_entry_detail(requested_date)))
}

fn read_limited(reader: &mut impl Read, limit: u64) -> CommandResult<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CommandError::invalid_backup())?;
    if bytes.len() as u64 > limit {
        return Err(CommandError::invalid_backup());
    }
    Ok(bytes)
}

fn validate_staged_asset(path: &Path, name: &str) -> CommandResult<()> {
    let detected = infer::get_from_path(path)
        .map_err(|_| CommandError::invalid_backup())?
        .ok_or_else(CommandError::invalid_backup)?;
    let extension_matches = match detected.mime_type() {
        "image/png" => name.ends_with(".png"),
        "image/jpeg" => name.ends_with(".jpg") || name.ends_with(".jpeg"),
        "image/webp" => name.ends_with(".webp"),
        _ => false,
    };
    if !extension_matches {
        return Err(CommandError::invalid_backup());
    }
    let (width, height) =
        image::image_dimensions(path).map_err(|_| CommandError::invalid_backup())?;
    if width > 4096 || height > 4096 || u64::from(width) * u64::from(height) > 20_000_000 {
        return Err(CommandError::invalid_backup());
    }
    Ok(())
}

fn validate_backup_data(manifest: &BackupManifest, data: &BackupData) -> CommandResult<()> {
    if manifest.format_version != BACKUP_FORMAT_VERSION
        || manifest.exported_at < 0
        || data.entries.len() > MAX_BACKUP_ENTRIES
    {
        return Err(CommandError::invalid_backup());
    }

    let mut entry_ids = BTreeSet::new();
    let mut entry_dates = BTreeSet::new();
    let mut markdown_references = HashMap::new();
    for entry in &data.entries {
        if !valid_date(&entry.entry_date)
            || !Uuid::parse_str(&entry.id).is_ok_and(|id| id.to_string() == entry.id)
            || !entry_ids.insert(entry.id.clone())
            || !entry_dates.insert(entry.entry_date.clone())
            || entry.content_md.len() > 200_000
            || entry.word_count < 0
            || entry.word_count != word_count(&entry.content_md)
            || entry.revision < 0
            || entry.created_at < 0
            || entry.updated_at < 0
        {
            return Err(CommandError::invalid_backup());
        }
        let references = markdown_asset_references(&entry.content_md)
            .map_err(|_| CommandError::invalid_backup())?;
        markdown_references.insert(entry.id.clone(), references);
    }

    let mut declared_references: HashMap<&str, BTreeSet<String>> = HashMap::new();
    let mut reference_pairs = BTreeSet::new();
    let mut referenced_assets = BTreeSet::new();
    for reference in &data.entry_assets {
        if !entry_ids.contains(&reference.entry_id)
            || !valid_original_asset_file_name(&reference.asset_name)
            || !reference_pairs.insert((reference.entry_id.clone(), reference.asset_name.clone()))
        {
            return Err(CommandError::invalid_backup());
        }
        declared_references
            .entry(&reference.entry_id)
            .or_default()
            .insert(reference.asset_name.clone());
        referenced_assets.insert(reference.asset_name.clone());
    }
    for entry in &data.entries {
        if markdown_references.get(&entry.id)
            != Some(declared_references.entry(&entry.id).or_default())
        {
            return Err(CommandError::invalid_backup());
        }
    }

    let mut manifest_assets = BTreeSet::new();
    for asset in &manifest.assets {
        if !valid_original_asset_file_name(&asset.name)
            || !manifest_assets.insert(asset.name.clone())
            || asset.size == 0
            || asset.size > MAX_BACKUP_ASSET_BYTES
            || asset.sha256.len() != 64
            || !asset
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(CommandError::invalid_backup());
        }
    }
    if manifest_assets != referenced_assets {
        return Err(CommandError::invalid_backup());
    }

    let mut setting_keys = BTreeSet::new();
    if data
        .user_settings
        .iter()
        .any(|setting| !valid_setting_key(&setting.key) || !setting_keys.insert(&setting.key))
    {
        return Err(CommandError::invalid_backup());
    }
    Ok(())
}

fn inspect_backup_archive(
    source: &Path,
    staging: &Path,
) -> CommandResult<(BackupManifest, BackupData)> {
    let mut archive =
        ZipArchive::new(fs::File::open(source).map_err(|_| CommandError::invalid_backup())?)
            .map_err(|_| CommandError::invalid_backup())?;
    if archive.len() > MAX_BACKUP_ENTRIES + 2 {
        return Err(CommandError::invalid_backup());
    }

    let mut names = BTreeSet::new();
    let mut total_size = 0_u64;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| CommandError::invalid_backup())?;
        let name = file.name();
        let valid_name = name == "manifest.json"
            || name == "entries.json"
            || name
                .strip_prefix("assets/")
                .is_some_and(valid_original_asset_file_name);
        if !valid_name
            || file.is_dir()
            || file.enclosed_name().is_none()
            || !names.insert(name.to_owned())
        {
            return Err(CommandError::invalid_backup());
        }
        let item_limit = if name.starts_with("assets/") {
            MAX_BACKUP_ASSET_BYTES
        } else {
            MAX_BACKUP_JSON_BYTES
        };
        if file.size() > item_limit {
            return Err(CommandError::invalid_backup());
        }
        total_size = total_size
            .checked_add(file.size())
            .ok_or_else(CommandError::invalid_backup)?;
        if total_size > MAX_BACKUP_TOTAL_BYTES {
            return Err(CommandError::invalid_backup());
        }
    }
    if !names.contains("manifest.json") || !names.contains("entries.json") {
        return Err(CommandError::invalid_backup());
    }

    let manifest: BackupManifest = {
        let mut file = archive
            .by_name("manifest.json")
            .map_err(|_| CommandError::invalid_backup())?;
        serde_json::from_slice(&read_limited(&mut file, MAX_BACKUP_JSON_BYTES)?)
            .map_err(|_| CommandError::invalid_backup())?
    };
    let data: BackupData = {
        let mut file = archive
            .by_name("entries.json")
            .map_err(|_| CommandError::invalid_backup())?;
        serde_json::from_slice(&read_limited(&mut file, MAX_BACKUP_JSON_BYTES)?)
            .map_err(|_| CommandError::invalid_backup())?
    };
    validate_backup_data(&manifest, &data)?;

    let assets_directory = staging.join("assets");
    fs::create_dir_all(&assets_directory).map_err(|_| CommandError::database())?;
    for item in &manifest.assets {
        let mut source_file = archive
            .by_name(&format!("assets/{}", item.name))
            .map_err(|_| CommandError::invalid_backup())?;
        let target_path = assets_directory.join(&item.name);
        let mut target_file =
            fs::File::create(&target_path).map_err(|_| CommandError::database())?;
        let mut hasher = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = source_file
                .read(&mut buffer)
                .map_err(|_| CommandError::invalid_backup())?;
            if read == 0 {
                break;
            }
            copied = copied
                .checked_add(read as u64)
                .ok_or_else(CommandError::invalid_backup)?;
            if copied > item.size || copied > MAX_BACKUP_ASSET_BYTES {
                return Err(CommandError::invalid_backup());
            }
            hasher.update(&buffer[..read]);
            target_file
                .write_all(&buffer[..read])
                .map_err(|_| CommandError::database())?;
        }
        target_file
            .sync_all()
            .map_err(|_| CommandError::database())?;
        if copied != item.size || format!("{:x}", hasher.finalize()) != item.sha256 {
            return Err(CommandError::invalid_backup());
        }
        validate_staged_asset(&target_path, &item.name)?;
    }
    Ok((manifest, data))
}

fn file_sha256(path: &Path) -> CommandResult<String> {
    let mut file = fs::File::open(path).map_err(|_| CommandError::database())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| CommandError::database())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_asset_collisions(
    assets_directory: &Path,
    manifest: &BackupManifest,
) -> CommandResult<()> {
    for asset in &manifest.assets {
        let target = assets_directory.join(&asset.name);
        if target.exists() && file_sha256(&target)? != asset.sha256 {
            return Err(CommandError::invalid_backup());
        }
    }
    Ok(())
}

fn verify_staged_assets(staging: &Path, manifest: &BackupManifest) -> CommandResult<()> {
    for asset in &manifest.assets {
        let path = staging.join("assets").join(&asset.name);
        let metadata = fs::metadata(&path).map_err(|_| CommandError::invalid_backup())?;
        if !metadata.is_file()
            || metadata.len() != asset.size
            || file_sha256(&path).map_err(|_| CommandError::invalid_backup())? != asset.sha256
        {
            return Err(CommandError::invalid_backup());
        }
        validate_staged_asset(&path, &asset.name)?;
    }
    Ok(())
}

fn take_pending_import(
    pending: &PendingImports,
    token: &str,
    now: i64,
) -> CommandResult<PendingImport> {
    let session = pending
        .0
        .lock()
        .map_err(|_| CommandError::database())?
        .remove(token)
        .ok_or_else(CommandError::invalid_import_token)?;
    if session.expires_at <= now {
        let _ = fs::remove_dir_all(&session.staging);
        return Err(CommandError::import_token_expired());
    }
    Ok(session)
}

fn apply_backup_to_database(
    connection: &mut Connection,
    data: &BackupData,
    mode: &str,
    staged_assets: &Path,
    assets_directory: &Path,
) -> CommandResult<usize> {
    let transaction = connection
        .transaction()
        .map_err(|_| CommandError::database())?;
    if mode == "replace_all" {
        transaction
            .execute("DELETE FROM entry_assets", [])
            .map_err(|_| CommandError::database())?;
        transaction
            .execute("DELETE FROM entries", [])
            .map_err(|_| CommandError::database())?;
        transaction
            .execute("DELETE FROM user_settings", [])
            .map_err(|_| CommandError::database())?;
    }

    let mut imported_dates = HashMap::new();
    for entry in &data.entries {
        let should_write = mode == "replace_all"
            || transaction
                .query_row(
                    "SELECT updated_at FROM entries WHERE entry_date = ?1",
                    params![entry.entry_date],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|_| CommandError::database())?
                .is_none_or(|updated_at| entry.updated_at > updated_at);
        if !should_write {
            continue;
        }
        imported_dates.insert(entry.id.as_str(), entry.entry_date.as_str());
        transaction
            .execute(
                "INSERT INTO entries (id, entry_date, is_ticked, content_md, word_count, revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(entry_date) DO UPDATE SET
                    is_ticked=excluded.is_ticked, content_md=excluded.content_md,
                    word_count=excluded.word_count, revision=excluded.revision,
                    created_at=excluded.created_at, updated_at=excluded.updated_at",
                params![
                    entry.id,
                    entry.entry_date,
                    entry.is_ticked as i64,
                    entry.content_md,
                    entry.word_count,
                    entry.revision,
                    entry.created_at,
                    entry.updated_at
                ],
            )
            .map_err(|_| CommandError::database())?;
    }

    for date in imported_dates.values() {
        transaction
            .execute(
                "DELETE FROM entry_assets WHERE entry_id = (SELECT id FROM entries WHERE entry_date = ?1)",
                params![date],
            )
            .map_err(|_| CommandError::database())?;
    }
    let mut required_assets = BTreeSet::new();
    for reference in &data.entry_assets {
        if let Some(date) = imported_dates.get(reference.entry_id.as_str()) {
            transaction
                .execute(
                    "INSERT INTO entry_assets (entry_id, asset_name)
                     SELECT id, ?1 FROM entries WHERE entry_date = ?2",
                    params![reference.asset_name, date],
                )
                .map_err(|_| CommandError::database())?;
            required_assets.insert(reference.asset_name.as_str());
        }
    }
    for setting in &data.user_settings {
        transaction
            .execute(
                "INSERT INTO user_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![setting.key, setting.value],
            )
            .map_err(|_| CommandError::database())?;
    }

    let mut installed = Vec::new();
    for name in required_assets {
        let target = assets_directory.join(name);
        if target.exists() {
            continue;
        }
        if fs::rename(staged_assets.join(name), &target).is_err() {
            for path in installed {
                let _ = fs::remove_file(path);
            }
            return Err(CommandError::database());
        }
        installed.push(target);
    }
    if transaction.commit().is_err() {
        for path in installed {
            let _ = fs::remove_file(path);
        }
        return Err(CommandError::database());
    }
    Ok(imported_dates.len())
}

fn controlled_temporary_path(app: &AppHandle, source_path: &str) -> CommandResult<PathBuf> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::database())?;
    let root = app_data.join("tmp");
    fs::create_dir_all(&root).map_err(|_| CommandError::database())?;
    let root = fs::canonicalize(root).map_err(|_| CommandError::invalid_backup())?;
    let requested = Path::new(source_path);
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        if !requested
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            return Err(CommandError::invalid_backup());
        }
        app_data.join(requested)
    };
    let source = fs::canonicalize(candidate).map_err(|_| CommandError::invalid_backup())?;
    if !source.starts_with(root)
        || source
            .extension()
            .is_none_or(|extension| extension != "zip")
    {
        return Err(CommandError::invalid_backup());
    }
    Ok(source)
}

#[tauri::command]
fn pick_backup_file(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(all(desktop, debug_assertions))]
    {
        use tauri_plugin_dialog::DialogExt;

        let selected = app
            .dialog()
            .file()
            .add_filter("勾勾备份", &["zip"])
            .blocking_pick_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let source = selected
            .into_path()
            .map_err(|_| CommandError::invalid_backup())?;
        if source
            .extension()
            .is_none_or(|extension| extension != "zip")
            || fs::metadata(&source)
                .map_err(|_| CommandError::invalid_backup())?
                .len()
                > MAX_BACKUP_TOTAL_BYTES
        {
            return Err(CommandError::invalid_backup());
        }
        let relative = format!("tmp/incoming/{}.zip", Uuid::new_v4());
        let target = app
            .path()
            .app_data_dir()
            .map_err(|_| CommandError::database())?
            .join(&relative);
        fs::create_dir_all(target.parent().expect("incoming backup has parent"))
            .map_err(|_| CommandError::database())?;
        fs::copy(source, target).map_err(|_| CommandError::database())?;
        return Ok(Some(relative));
    }

    #[cfg(not(all(desktop, debug_assertions)))]
    {
        let _ = app;
        Err(CommandError::unsupported_platform())
    }
}

#[tauri::command]
fn save_backup_copy(app: AppHandle, source_path: String) -> CommandResult<bool> {
    #[cfg(all(desktop, debug_assertions))]
    {
        use tauri_plugin_dialog::DialogExt;

        let source = controlled_temporary_path(&app, &source_path)?;
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(CommandError::invalid_backup)?;
        let target = app
            .dialog()
            .file()
            .add_filter("勾勾备份", &["zip"])
            .set_file_name(file_name)
            .blocking_save_file();
        let Some(target) = target else {
            return Ok(false);
        };
        let target = target
            .into_path()
            .map_err(|_| CommandError::invalid_backup())?;
        fs::copy(source, target).map_err(|_| CommandError::database())?;
        return Ok(true);
    }

    #[cfg(not(all(desktop, debug_assertions)))]
    {
        let _ = (app, source_path);
        Err(CommandError::unsupported_platform())
    }
}

#[tauri::command]
fn inspect_backup(
    app: AppHandle,
    source_path: String,
    database: State<'_, Database>,
    pending: State<'_, PendingImports>,
) -> CommandResult<BackupPreview> {
    let source = controlled_temporary_path(&app, &source_path)?;
    let token = Uuid::new_v4().to_string();
    let staging = import_staging_directory(&app, &token)?;
    let inspected = inspect_backup_archive(&source, &staging);
    let (manifest, data) = match inspected {
        Ok(inspected) => inspected,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    let conflict_count = {
        let connection = database.0.lock().map_err(|_| CommandError::database())?;
        data.entries.iter().try_fold(0_usize, |count, entry| {
            let exists = connection
                .query_row(
                    "SELECT 1 FROM entries WHERE entry_date = ?1",
                    params![entry.entry_date],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|_| CommandError::database())?
                .is_some();
            Ok::<_, CommandError>(count + usize::from(exists))
        })?
    };
    let entry_count = data.entries.len();
    let asset_count = manifest.assets.len();
    let expires_at = now_unix_seconds()? + IMPORT_TOKEN_TTL_SECONDS;
    let mut sessions = pending.0.lock().map_err(|_| CommandError::database())?;
    let now = now_unix_seconds()?;
    let expired_directories: Vec<_> = sessions
        .extract_if(|_, session| session.expires_at <= now)
        .map(|(_, session)| session.staging)
        .collect();
    sessions.insert(
        token.clone(),
        PendingImport {
            staging,
            expires_at,
            manifest,
            data,
        },
    );
    drop(sessions);
    for directory in expired_directories {
        let _ = fs::remove_dir_all(directory);
    }
    Ok(BackupPreview {
        import_token: token,
        entry_count,
        asset_count,
        conflict_count,
    })
}

#[tauri::command]
fn apply_backup(
    app: AppHandle,
    import_token: String,
    mode: String,
    database: State<'_, Database>,
    pending: State<'_, PendingImports>,
) -> CommandResult<BackupImport> {
    if mode != "merge_newer" && mode != "replace_all" {
        return Err(CommandError::invalid_import_mode());
    }
    let session = take_pending_import(&pending, &import_token, now_unix_seconds()?)?;
    let assets_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::database())?
        .join("assets");
    fs::create_dir_all(&assets_dir).map_err(|_| CommandError::database())?;
    let result = (|| {
        verify_staged_assets(&session.staging, &session.manifest)?;
        validate_asset_collisions(&assets_dir, &session.manifest)?;
        let imported = {
            let mut connection = database.0.lock().map_err(|_| CommandError::database())?;
            apply_backup_to_database(
                &mut connection,
                &session.data,
                &mode,
                &session.staging.join("assets"),
                &assets_dir,
            )?
        };
        let referenced = {
            let connection = database.0.lock().map_err(|_| CommandError::database())?;
            referenced_assets(&connection)?
        };
        let _ = cleanup_unreferenced_assets(&app, &referenced);
        Ok(BackupImport {
            imported_entries: imported,
        })
    })();
    let _ = fs::remove_dir_all(&session.staging);
    result
}

#[tauri::command]
fn export_backup(app: AppHandle, database: State<'_, Database>) -> CommandResult<BackupExport> {
    let snapshot = {
        let connection = database.0.lock().map_err(|_| CommandError::database())?;
        backup_snapshot(&connection)?
    };
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::database())?;
    let assets_dir = app_data_dir.join("assets");
    let mut assets = Vec::new();
    for reference in &snapshot.entry_assets {
        let name = &reference.asset_name;
        if !valid_original_asset_file_name(name) {
            return Err(CommandError::database());
        }
        let bytes = fs::read(assets_dir.join(name)).map_err(|_| CommandError::database())?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        assets.push((name.clone(), bytes, sha256));
    }
    assets.sort_by(|left, right| left.0.cmp(&right.0));
    assets.dedup_by(|left, right| left.0 == right.0);

    let file_name = format!("gougou-backup-{}.zip", Local::now().format("%Y%m%d"));
    let final_path = backup_directory(&app)?.join(&file_name);
    let temporary_path = final_path.with_extension("zip.partial");
    let file = fs::File::create(&temporary_path).map_err(|_| CommandError::database())?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        exported_at: now_unix_seconds()?,
        assets: assets
            .iter()
            .map(|(name, bytes, sha256)| BackupAssetManifest {
                name: name.clone(),
                sha256: sha256.clone(),
                size: bytes.len() as u64,
            })
            .collect(),
    };
    let entries_json = serde_json::to_vec(&snapshot).map_err(|_| CommandError::database())?;
    let manifest_json = serde_json::to_vec(&manifest).map_err(|_| CommandError::database())?;
    let write_result = (|| {
        archive
            .start_file("manifest.json", options)
            .map_err(|_| CommandError::database())?;
        archive
            .write_all(&manifest_json)
            .map_err(|_| CommandError::database())?;
        archive
            .start_file("entries.json", options)
            .map_err(|_| CommandError::database())?;
        archive
            .write_all(&entries_json)
            .map_err(|_| CommandError::database())?;
        for (name, bytes, _) in &assets {
            archive
                .start_file(format!("assets/{name}"), options)
                .map_err(|_| CommandError::database())?;
            archive
                .write_all(bytes)
                .map_err(|_| CommandError::database())?;
        }
        archive.finish().map_err(|_| CommandError::database())?;
        fs::rename(&temporary_path, &final_path).map_err(|_| CommandError::database())?;
        Ok::<_, CommandError>(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(BackupExport {
        source_path: format!("tmp/exports/{file_name}"),
        file_name,
        entry_count: snapshot.entries.len(),
        asset_count: assets.len(),
    })
}

#[tauri::command]
fn get_month_entries(
    month: String,
    database: State<'_, Database>,
) -> CommandResult<Vec<MonthEntrySummary>> {
    let (month_start, next_month_start) =
        month_range(&month).ok_or_else(CommandError::invalid_date)?;
    let connection = database.0.lock().map_err(|_| CommandError::database())?;
    let mut statement = connection
        .prepare(
            "SELECT entry_date, is_ticked, content_md <> '', updated_at
             FROM entries
             WHERE entry_date >= ?1 AND entry_date < ?2
             ORDER BY entry_date",
        )
        .map_err(|_| CommandError::database())?;
    let summaries = statement
        .query_map(params![month_start, next_month_start], |row| {
            Ok(MonthEntrySummary {
                entry_date: row.get(0)?,
                is_ticked: row.get::<_, i64>(1)? != 0,
                has_content: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|_| CommandError::database())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CommandError::database())?;
    Ok(summaries)
}

#[tauri::command]
fn get_entry_detail(date: String, database: State<'_, Database>) -> CommandResult<EntryDetail> {
    if !valid_date(&date) {
        return Err(CommandError::invalid_date());
    }

    let connection = database.0.lock().map_err(|_| CommandError::database())?;
    read_entry_detail(&connection, date)
}

fn save_entry_to_database(
    connection: &mut Connection,
    date: String,
    content_md: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    if !valid_date(&date) {
        return Err(CommandError::invalid_date());
    }
    if expected_revision < 0 {
        return Err(CommandError::invalid_revision());
    }
    if content_md.len() > 200_000 {
        return Err(CommandError::content_too_large());
    }

    let asset_references = markdown_asset_references(&content_md)?;
    let now = now_unix_seconds()?;
    let content_word_count = word_count(&content_md);
    let transaction = connection
        .transaction()
        .map_err(|_| CommandError::database())?;
    let existing = transaction
        .query_row(
            "SELECT id, is_ticked, revision FROM entries WHERE entry_date = ?1",
            params![date],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? != 0,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| CommandError::database())?;

    match existing {
        Some((entry_id, is_ticked, revision)) => {
            if revision != expected_revision {
                return Err(CommandError::revision_conflict());
            }
            if content_md.is_empty() && !is_ticked {
                transaction
                    .execute(
                        "DELETE FROM entries WHERE entry_date = ?1 AND revision = ?2",
                        params![date, expected_revision],
                    )
                    .map_err(|_| CommandError::database())?;
                transaction.commit().map_err(|_| CommandError::database())?;
                return Ok(empty_entry_detail(date));
            }

            let changed = transaction
                .execute(
                    "UPDATE entries
                     SET content_md = ?1, word_count = ?2, revision = revision + 1, updated_at = ?3
                     WHERE entry_date = ?4 AND revision = ?5",
                    params![content_md, content_word_count, now, date, expected_revision],
                )
                .map_err(|_| CommandError::database())?;
            if changed != 1 {
                return Err(CommandError::revision_conflict());
            }
            replace_entry_assets(&transaction, &entry_id, &asset_references)?;
        }
        None => {
            if expected_revision != 0 {
                return Err(CommandError::revision_conflict());
            }
            if content_md.is_empty() {
                transaction.commit().map_err(|_| CommandError::database())?;
                return Ok(empty_entry_detail(date));
            }

            let entry_id = Uuid::new_v4().to_string();
            transaction
                .execute(
                    "INSERT INTO entries (id, entry_date, content_md, word_count, revision, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
                    params![
                        &entry_id,
                        date,
                        content_md,
                        content_word_count,
                        now
                    ],
                )
                .map_err(|_| CommandError::database())?;
            replace_entry_assets(&transaction, &entry_id, &asset_references)?;
        }
    }

    transaction.commit().map_err(|_| CommandError::database())?;
    read_entry_detail(connection, date)
}

#[tauri::command]
fn save_entry(
    app: AppHandle,
    date: String,
    content_md: String,
    expected_revision: i64,
    database: State<'_, Database>,
) -> CommandResult<EntryDetail> {
    let (detail, referenced) = {
        let mut connection = database.0.lock().map_err(|_| CommandError::database())?;
        let detail = save_entry_to_database(&mut connection, date, content_md, expected_revision)?;
        let referenced = referenced_assets(&connection)?;
        (detail, referenced)
    };
    let _ = cleanup_unreferenced_assets(&app, &referenced);
    Ok(detail)
}

#[tauri::command]
fn toggle_tick(date: String, database: State<'_, Database>) -> CommandResult<MonthEntrySummary> {
    if !valid_date(&date) {
        return Err(CommandError::invalid_date());
    }

    let now = now_unix_seconds()?;
    let id = Uuid::new_v4().to_string();
    let summary = {
        let connection = database.0.lock().map_err(|_| CommandError::database())?;
        connection
            .execute(
                "INSERT INTO entries (id, entry_date, is_ticked, created_at, updated_at)
                 VALUES (?1, ?2, 1, ?3, ?3)
                 ON CONFLICT(entry_date) DO UPDATE SET
                     is_ticked = CASE entries.is_ticked WHEN 1 THEN 0 ELSE 1 END,
                     updated_at = excluded.updated_at",
                params![id, date, now],
            )
            .map_err(|_| CommandError::database())?;

        connection
            .query_row(
                "SELECT entry_date, is_ticked, content_md <> '', updated_at FROM entries WHERE entry_date = ?1",
                params![date],
                |row| {
                    Ok(MonthEntrySummary {
                        entry_date: row.get(0)?,
                        is_ticked: row.get::<_, i64>(1)? != 0,
                        has_content: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .map_err(|_| CommandError::database())?
    };
    Ok(summary)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .register_uri_scheme_protocol("gougou-asset", |context, request| {
            let name = request.uri().path().trim_start_matches('/');
            if !valid_asset_file_name(name) {
                return tauri::http::Response::builder()
                    .status(tauri::http::StatusCode::NOT_FOUND)
                    .body(Vec::new())
                    .expect("valid protocol response");
            }
            let response = context
                .app_handle()
                .path()
                .app_data_dir()
                .ok()
                .and_then(|directory| fs::read(directory.join("assets").join(name)).ok());
            match response {
                Some(bytes) => tauri::http::Response::builder()
                    .header(tauri::http::header::CONTENT_TYPE, asset_content_type(name))
                    .body(bytes)
                    .expect("valid protocol response"),
                None => tauri::http::Response::builder()
                    .status(tauri::http::StatusCode::NOT_FOUND)
                    .body(Vec::new())
                    .expect("valid protocol response"),
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_gougou_reminder::init());
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_biometric::init());
    builder
        .setup(|app| {
            cleanup_expired_temporary_files(app.handle());
            let connection =
                open_database(app.handle()).map_err(|error| std::io::Error::other(error))?;
            #[cfg(mobile)]
            let _ = sync_reminder_from_database(app.handle(), &connection);
            app.manage(Database(Mutex::new(connection)));
            app.manage(PendingImports(Mutex::new(HashMap::new())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            health_check,
            get_app_settings,
            get_biometric_platform_status,
            update_app_settings,
            get_reminder_status,
            take_reminder_target,
            request_reminder_permission,
            sync_reminder,
            save_backup_copy,
            pick_backup_file,
            pick_and_import_image,
            export_backup,
            apply_backup,
            inspect_backup,
            import_image,
            get_month_entries,
            get_entry_detail,
            save_entry,
            toggle_tick
        ])
        .run(tauri::generate_context!())
        .expect("error while running Gougou");
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, io::Write, path::PathBuf, sync::Mutex};

    use image::{ImageBuffer, Rgb};
    use rusqlite::{params, Connection};
    use sha2::{Digest, Sha256};
    use uuid::Uuid;
    use zip::{write::SimpleFileOptions, ZipWriter};

    use super::{
        apply_backup_to_database, database_health, get_entry_detail, get_month_entries,
        health_check, inspect_backup_archive, markdown_asset_references, migrate, month_range,
        read_app_settings, read_entry_detail, save_entry_to_database, take_pending_import,
        toggle_tick, valid_date, validate_app_settings, validate_backup_data, word_count,
        write_app_settings, AppSettings, BackupAssetManifest, BackupAssetReference, BackupData,
        BackupEntry, BackupManifest, BackupSetting, Database, PendingImport, PendingImports,
        BACKUP_FORMAT_VERSION,
    };

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("gougou-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        connection
    }

    fn empty_backup() -> (BackupManifest, BackupData) {
        (
            BackupManifest {
                format_version: BACKUP_FORMAT_VERSION,
                exported_at: 1,
                assets: Vec::new(),
            },
            BackupData {
                entries: Vec::new(),
                entry_assets: Vec::new(),
                user_settings: Vec::new(),
            },
        )
    }

    fn write_backup(
        path: &PathBuf,
        manifest: &BackupManifest,
        data: &BackupData,
        assets: &[(&str, &[u8])],
        extra_entry: Option<(&str, &[u8])>,
    ) {
        let mut archive = ZipWriter::new(fs::File::create(path).expect("create backup"));
        let options = SimpleFileOptions::default();
        archive
            .start_file("manifest.json", options)
            .expect("manifest file");
        archive
            .write_all(&serde_json::to_vec(manifest).expect("serialize manifest"))
            .expect("write manifest");
        archive
            .start_file("entries.json", options)
            .expect("entries file");
        archive
            .write_all(&serde_json::to_vec(data).expect("serialize entries"))
            .expect("write entries");
        for (name, bytes) in assets {
            archive
                .start_file(format!("assets/{name}"), options)
                .expect("asset file");
            archive.write_all(bytes).expect("write asset");
        }
        if let Some((name, bytes)) = extra_entry {
            archive.start_file(name, options).expect("extra file");
            archive.write_all(bytes).expect("write extra");
        }
        archive.finish().expect("finish backup");
    }

    fn png_bytes(directory: &TestDirectory) -> Vec<u8> {
        let path = directory.0.join("pixel.png");
        ImageBuffer::<Rgb<u8>, _>::from_pixel(1, 1, Rgb([20, 40, 60]))
            .save(&path)
            .expect("save png");
        fs::read(path).expect("read png")
    }

    fn backup_entry(id: &str, date: &str, content: &str, updated_at: i64) -> BackupEntry {
        BackupEntry {
            id: id.into(),
            entry_date: date.into(),
            is_ticked: false,
            content_md: content.into(),
            word_count: word_count(content),
            revision: 1,
            created_at: 1,
            updated_at,
        }
    }

    #[test]
    fn validates_civil_dates() {
        assert!(valid_date("2026-02-28"));
        assert!(valid_date("2024-02-29"));
        assert!(!valid_date("2026-02-29"));
        assert!(!valid_date("2026-2-01"));
    }

    #[test]
    fn reports_database_health() {
        let connection = test_connection();
        let health = database_health(&connection).expect("healthy database");
        assert!(health.database_ready);
        assert_eq!(health.schema_version, 1);
    }

    #[test]
    fn app_settings_default_and_round_trip() {
        let mut connection = test_connection();
        let defaults = read_app_settings(&connection).expect("read default settings");
        assert_eq!(defaults, AppSettings::default());

        let mut changed = defaults;
        changed.reminder.enabled = true;
        changed.reminder.hour = 20;
        changed.reminder.minute = 30;
        changed.reminder.quiet_weekdays = vec![6, 7];
        changed.reminder.paused_until = Some("2026-07-19".into());
        changed.privacy.lock_enabled = true;
        changed.appearance.theme = "dark".into();
        changed.accessibility.reduce_motion = true;
        changed.accessibility.haptics = false;
        write_app_settings(&mut connection, &changed).expect("write settings");
        assert_eq!(
            read_app_settings(&connection).expect("read settings"),
            changed
        );
    }

    #[test]
    fn rejects_invalid_and_unknown_settings() {
        let mut invalid = AppSettings::default();
        invalid.reminder.hour = 24;
        assert!(validate_app_settings(&invalid).is_err());
        invalid = AppSettings::default();
        invalid.reminder.quiet_weekdays = vec![1, 1];
        assert!(validate_app_settings(&invalid).is_err());
        invalid = AppSettings::default();
        invalid.appearance.theme = "neon".into();
        assert!(validate_app_settings(&invalid).is_err());

        let (manifest, mut data) = empty_backup();
        data.user_settings.push(BackupSetting {
            key: "unknown.setting".into(),
            value: "true".into(),
        });
        assert!(validate_backup_data(&manifest, &data).is_err());
    }

    #[test]
    fn ipc_health_read_and_tick_commands_share_database_state() {
        use tauri::{ipc::CallbackFn, test, webview::InvokeRequest, WebviewWindowBuilder};

        let app = test::mock_builder()
            .manage(Database(Mutex::new(test_connection())))
            .invoke_handler(tauri::generate_handler![
                health_check,
                get_month_entries,
                get_entry_detail,
                toggle_tick
            ])
            .build(test::mock_context(test::noop_assets()))
            .expect("build mock app");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("build mock webview");
        let request = |command: &str, body: serde_json::Value| InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("valid invoke URL"),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: test::INVOKE_KEY.into(),
        };

        let health =
            test::get_ipc_response(&webview, request("health_check", serde_json::json!({})))
                .expect("health IPC response")
                .deserialize::<serde_json::Value>()
                .expect("deserialize health");
        assert_eq!(health["databaseReady"], true);

        let ticked = test::get_ipc_response(
            &webview,
            request("toggle_tick", serde_json::json!({ "date": "2026-07-12" })),
        )
        .expect("tick IPC response")
        .deserialize::<serde_json::Value>()
        .expect("deserialize tick");
        assert_eq!(ticked["entryDate"], "2026-07-12");
        assert_eq!(ticked["isTicked"], true);

        let entries = test::get_ipc_response(
            &webview,
            request(
                "get_month_entries",
                serde_json::json!({ "month": "2026-07" }),
            ),
        )
        .expect("month IPC response")
        .deserialize::<serde_json::Value>()
        .expect("deserialize month entries");
        assert_eq!(entries.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn creates_month_boundaries() {
        assert_eq!(
            month_range("2026-07"),
            Some(("2026-07".into(), "2026-08".into()))
        );
        assert_eq!(
            month_range("2026-12"),
            Some(("2026-12".into(), "2027-01".into()))
        );
    }

    #[test]
    fn counts_visible_cjk_and_latin_words() {
        assert_eq!(word_count("# 你好，hello 123\n\n世界"), 6);
    }

    #[test]
    fn saves_with_revision_and_rejects_stale_content() {
        let mut connection = test_connection();
        let saved = save_entry_to_database(
            &mut connection,
            "2026-07-11".into(),
            "你好，hello".into(),
            0,
        )
        .expect("save entry");
        assert!(saved.exists);
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.word_count, 3);

        let stale =
            save_entry_to_database(&mut connection, "2026-07-11".into(), "旧内容".into(), 0)
                .expect_err("reject stale revision");
        assert_eq!(stale.code, "revision_conflict");
    }

    #[test]
    fn removes_unticked_empty_entries_but_keeps_ticked_ones() {
        let mut connection = test_connection();
        let saved = save_entry_to_database(&mut connection, "2026-07-11".into(), "内容".into(), 0)
            .expect("save entry");
        let deleted = save_entry_to_database(
            &mut connection,
            "2026-07-11".into(),
            String::new(),
            saved.revision,
        )
        .expect("delete empty entry");
        assert!(!deleted.exists);
        assert_eq!(deleted.revision, 0);
        assert!(
            !read_entry_detail(&connection, "2026-07-11".into())
                .expect("read empty entry")
                .exists
        );

        let saved = save_entry_to_database(&mut connection, "2026-07-11".into(), "内容".into(), 0)
            .expect("save entry again");
        connection
            .execute(
                "UPDATE entries SET is_ticked = 1 WHERE entry_date = '2026-07-11'",
                [],
            )
            .expect("tick entry");
        let retained = save_entry_to_database(
            &mut connection,
            "2026-07-11".into(),
            String::new(),
            saved.revision,
        )
        .expect("retain ticked entry");
        assert!(retained.exists);
        assert!(retained.is_ticked);
        assert_eq!(retained.content_md, "");
    }

    #[test]
    fn tracks_only_controlled_shared_image_references() {
        let image = "assets/12345678-1234-1234-1234-123456789abc.jpg";
        assert!(markdown_asset_references(&format!("![]({image})")).is_ok());
        assert!(markdown_asset_references("![](file:///private/photo.jpg)").is_err());

        let mut connection = test_connection();
        let first = save_entry_to_database(
            &mut connection,
            "2026-07-11".into(),
            format!("![]({image})"),
            0,
        )
        .expect("save first");
        save_entry_to_database(
            &mut connection,
            "2026-07-12".into(),
            format!("![]({image})"),
            0,
        )
        .expect("save second");
        let references: i64 = connection
            .query_row("SELECT COUNT(*) FROM entry_assets", [], |row| row.get(0))
            .expect("count references");
        assert_eq!(references, 2);
        save_entry_to_database(
            &mut connection,
            "2026-07-11".into(),
            "文字".into(),
            first.revision,
        )
        .expect("remove first reference");
        let references: i64 = connection
            .query_row("SELECT COUNT(*) FROM entry_assets", [], |row| row.get(0))
            .expect("count remaining references");
        assert_eq!(references, 1);
    }

    #[test]
    fn inspects_empty_backup_and_rejects_path_traversal() {
        let directory = TestDirectory::new();
        let source = directory.0.join("backup.zip");
        let staging = directory.0.join("staging");
        let (manifest, data) = empty_backup();
        write_backup(&source, &manifest, &data, &[], None);
        let inspected = inspect_backup_archive(&source, &staging).expect("inspect empty backup");
        assert!(inspected.0.assets.is_empty());
        assert!(inspected.1.entries.is_empty());

        let malicious = directory.0.join("malicious.zip");
        write_backup(
            &malicious,
            &manifest,
            &data,
            &[],
            Some(("../outside", b"no")),
        );
        let error = inspect_backup_archive(&malicious, &directory.0.join("bad-staging"))
            .expect_err("reject traversal");
        assert_eq!(error.code, "invalid_backup");
    }

    #[test]
    fn verifies_asset_hash_and_accepts_shared_references() {
        let directory = TestDirectory::new();
        let bytes = png_bytes(&directory);
        let asset_name = "12345678-1234-4234-8234-123456789abc.png";
        let markdown = format!("![](assets/{asset_name})");
        let first_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let second_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let data = BackupData {
            entries: vec![
                backup_entry(first_id, "2026-07-11", &markdown, 1),
                backup_entry(second_id, "2026-07-12", &markdown, 1),
            ],
            entry_assets: vec![
                BackupAssetReference {
                    entry_id: first_id.into(),
                    asset_name: asset_name.into(),
                },
                BackupAssetReference {
                    entry_id: second_id.into(),
                    asset_name: asset_name.into(),
                },
            ],
            user_settings: Vec::new(),
        };
        let mut manifest = BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            exported_at: 1,
            assets: vec![BackupAssetManifest {
                name: asset_name.into(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                size: bytes.len() as u64,
            }],
        };
        let source = directory.0.join("shared.zip");
        write_backup(&source, &manifest, &data, &[(asset_name, &bytes)], None);
        inspect_backup_archive(&source, &directory.0.join("shared-staging"))
            .expect("accept shared asset");

        manifest.assets[0].sha256 = "0".repeat(64);
        let corrupt = directory.0.join("corrupt.zip");
        write_backup(&corrupt, &manifest, &data, &[(asset_name, &bytes)], None);
        let error = inspect_backup_archive(&corrupt, &directory.0.join("corrupt-staging"))
            .expect_err("reject wrong hash");
        assert_eq!(error.code, "invalid_backup");
    }

    #[test]
    fn merge_keeps_newer_local_entry_and_imports_new_date() {
        let directory = TestDirectory::new();
        let staged = directory.0.join("staged");
        let assets = directory.0.join("assets");
        fs::create_dir_all(&staged).expect("create staging");
        fs::create_dir_all(&assets).expect("create assets");
        let mut connection = test_connection();
        connection
            .execute(
                "INSERT INTO entries (id, entry_date, content_md, word_count, revision, created_at, updated_at)
                 VALUES (?1, '2026-07-11', '本地较新', 4, 1, 1, 200)",
                params!["cccccccc-cccc-4ccc-8ccc-cccccccccccc"],
            )
            .expect("insert local entry");
        let data = BackupData {
            entries: vec![
                backup_entry(
                    "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                    "2026-07-11",
                    "备份较旧",
                    100,
                ),
                backup_entry(
                    "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                    "2026-07-12",
                    "新日期",
                    100,
                ),
            ],
            entry_assets: Vec::new(),
            user_settings: vec![BackupSetting {
                key: "appearance".into(),
                value: "dark".into(),
            }],
        };
        let imported =
            apply_backup_to_database(&mut connection, &data, "merge_newer", &staged, &assets)
                .expect("merge backup");
        assert_eq!(imported, 1);
        let local: String = connection
            .query_row(
                "SELECT content_md FROM entries WHERE entry_date = '2026-07-11'",
                [],
                |row| row.get(0),
            )
            .expect("read local entry");
        assert_eq!(local, "本地较新");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .expect("count entries");
        assert_eq!(count, 2);
    }

    #[test]
    fn failed_asset_install_rolls_back_replace_all() {
        let directory = TestDirectory::new();
        let staged = directory.0.join("staged");
        let assets = directory.0.join("assets");
        fs::create_dir_all(&staged).expect("create empty staging");
        fs::create_dir_all(&assets).expect("create assets");
        let mut connection = test_connection();
        connection
            .execute(
                "INSERT INTO entries (id, entry_date, content_md, word_count, revision, created_at, updated_at)
                 VALUES (?1, '2026-07-10', '原内容', 3, 1, 1, 1)",
                params!["cccccccc-cccc-4ccc-8ccc-cccccccccccc"],
            )
            .expect("insert original");
        let asset_name = "12345678-1234-4234-8234-123456789abc.png";
        let markdown = format!("![](assets/{asset_name})");
        let entry_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let data = BackupData {
            entries: vec![backup_entry(entry_id, "2026-07-11", &markdown, 2)],
            entry_assets: vec![BackupAssetReference {
                entry_id: entry_id.into(),
                asset_name: asset_name.into(),
            }],
            user_settings: Vec::new(),
        };
        let error =
            apply_backup_to_database(&mut connection, &data, "replace_all", &staged, &assets)
                .expect_err("asset move must fail");
        assert_eq!(error.code, "database_error");
        let content: String = connection
            .query_row(
                "SELECT content_md FROM entries WHERE entry_date = '2026-07-10'",
                [],
                |row| row.get(0),
            )
            .expect("original entry remains");
        assert_eq!(content, "原内容");
    }

    #[test]
    fn import_tokens_are_single_use_and_expire() {
        let directory = TestDirectory::new();
        let (manifest, data) = empty_backup();
        let pending = PendingImports(Mutex::new(HashMap::from([(
            "once".into(),
            PendingImport {
                staging: directory.0.join("once"),
                expires_at: 20,
                manifest,
                data,
            },
        )])));
        take_pending_import(&pending, "once", 10).expect("consume valid token");
        let duplicate = take_pending_import(&pending, "once", 10).expect_err("reject reuse");
        assert_eq!(duplicate.code, "invalid_import_token");

        let (manifest, data) = empty_backup();
        pending.0.lock().expect("lock sessions").insert(
            "expired".into(),
            PendingImport {
                staging: directory.0.join("expired"),
                expires_at: 10,
                manifest,
                data,
            },
        );
        let expired =
            take_pending_import(&pending, "expired", 10).expect_err("reject expired token");
        assert_eq!(expired.code, "import_token_expired");
    }
}
