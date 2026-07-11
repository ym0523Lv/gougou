use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use pulldown_cmark::{Event, Parser};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 1;

struct Database(Mutex<Connection>);

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

    let now = now_unix_seconds()?;
    let content_word_count = word_count(&content_md);
    let transaction = connection
        .transaction()
        .map_err(|_| CommandError::database())?;
    let existing = transaction
        .query_row(
            "SELECT is_ticked, revision FROM entries WHERE entry_date = ?1",
            params![date],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|_| CommandError::database())?;

    match existing {
        Some((is_ticked, revision)) => {
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
        }
        None => {
            if expected_revision != 0 {
                return Err(CommandError::revision_conflict());
            }
            if content_md.is_empty() {
                transaction.commit().map_err(|_| CommandError::database())?;
                return Ok(empty_entry_detail(date));
            }

            transaction
                .execute(
                    "INSERT INTO entries (id, entry_date, content_md, word_count, revision, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
                    params![
                        Uuid::new_v4().to_string(),
                        date,
                        content_md,
                        content_word_count,
                        now
                    ],
                )
                .map_err(|_| CommandError::database())?;
        }
    }

    transaction.commit().map_err(|_| CommandError::database())?;
    read_entry_detail(connection, date)
}

#[tauri::command]
fn save_entry(
    date: String,
    content_md: String,
    expected_revision: i64,
    database: State<'_, Database>,
) -> CommandResult<EntryDetail> {
    let mut connection = database.0.lock().map_err(|_| CommandError::database())?;
    save_entry_to_database(&mut connection, date, content_md, expected_revision)
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
        .invoke_handler(tauri::generate_handler![
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
    use rusqlite::Connection;

    use super::{
        migrate, month_range, read_entry_detail, save_entry_to_database, valid_date, word_count,
    };

    fn test_connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        connection
    }

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
}
