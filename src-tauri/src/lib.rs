use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 1;

struct Database(Mutex<Connection>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MonthEntrySummary {
    entry_date: String,
    is_ticked: bool,
    has_content: bool,
    updated_at: i64,
}

#[derive(Serialize)]
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
fn toggle_tick(date: String, database: State<'_, Database>) -> CommandResult<MonthEntrySummary> {
    if !valid_date(&date) {
        return Err(CommandError::invalid_date());
    }

    let now = now_unix_seconds()?;
    let id = Uuid::new_v4().to_string();
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
        .map_err(|_| CommandError::database())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let connection =
                open_database(app.handle()).map_err(|error| std::io::Error::other(error))?;
            app.manage(Database(Mutex::new(connection)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_month_entries, toggle_tick])
        .run(tauri::generate_context!())
        .expect("error while running Gougou");
}

#[cfg(test)]
mod tests {
    use super::{month_range, valid_date};

    #[test]
    fn validates_civil_dates() {
        assert!(valid_date("2026-02-28"));
        assert!(valid_date("2024-02-29"));
        assert!(!valid_date("2026-02-29"));
        assert!(!valid_date("2026-2-01"));
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
}
