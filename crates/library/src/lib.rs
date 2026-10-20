// SPDX-License-Identifier: GPL-3.0-or-later
//! Durable PGN imports, analysis results, and the active review position.
//!
//! A `Library` owns one SQLite connection. Callers should keep it on their database
//! worker thread rather than performing disk access on the UI thread.

use std::{path::Path, time::Duration};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const SCHEMA_VERSION: i64 = 1;
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_GAMES: usize = 256;
const MAX_PLY: usize = 2048;

const SCHEMA: &str = "
CREATE TABLE imports (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,
    pgn TEXT NOT NULL UNIQUE CHECK(length(CAST(pgn AS BLOB)) <= 8388608),
    game_count INTEGER NOT NULL CHECK(game_count BETWEEN 1 AND 256)
);
CREATE TABLE games (
    id INTEGER PRIMARY KEY,
    import_id INTEGER NOT NULL REFERENCES imports(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 255),
    UNIQUE(import_id, ordinal)
);
CREATE TABLE analyses (
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    signature TEXT NOT NULL,
    ply INTEGER NOT NULL CHECK(ply BETWEEN 0 AND 2048),
    payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 8388608),
    PRIMARY KEY(game_id, signature, ply)
);
CREATE TABLE active_session (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    ply INTEGER NOT NULL CHECK(ply BETWEEN 0 AND 2048)
);
PRAGMA user_version = 1;
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredImport {
    pub id: i64,
    pub source: String,
    pub pgn: String,
    pub game_ids: Vec<i64>,
}

pub struct Library {
    connection: Connection,
}

impl Library {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut connection = Connection::open(path).map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        // Inspect the version before any persistent PRAGMA or schema changes.
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(db_error)?;
        validate_version(version)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(db_error)?;
        if version == 0 {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            // Another opener may have initialized the database while we waited
            // for this write lock. Decide whether to initialize under the lock.
            let locked_version: i64 = transaction
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .map_err(db_error)?;
            validate_version(locked_version)?;
            if locked_version == 0 {
                let table_count: i64 = transaction
                    .query_row(
                        "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(db_error)?;
                if table_count != 0 {
                    return Err(
                        "Unrecognized database: schema version is zero but it is not empty".into(),
                    );
                }
                transaction.execute_batch(SCHEMA).map_err(db_error)?;
            }
            transaction.commit().map_err(db_error)?;
        }
        validate_schema(&connection)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(db_error)?;
        Ok(Self { connection })
    }

    /// Import all games atomically. Identical PGN content reuses its original IDs.
    pub fn import(
        &mut self,
        source: &str,
        pgn: &str,
        game_count: usize,
    ) -> Result<Vec<i64>, String> {
        validate_text(pgn, "PGN")?;
        if !(1..=MAX_GAMES).contains(&game_count) {
            return Err(format!("An import must contain 1..={MAX_GAMES} games"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let existing: Option<(i64, usize)> = transaction
            .query_row(
                "SELECT id, game_count FROM imports WHERE pgn = ?1",
                [pgn],
                |row| Ok((row.get(0)?, bounded_usize(row, 1, 1, MAX_GAMES)?)),
            )
            .optional()
            .map_err(db_error)?;
        if let Some((id, previous_count)) = existing {
            if previous_count != game_count {
                return Err(
                    "Identical PGN was previously imported with a different game count".into(),
                );
            }
            let ids = game_ids(&transaction, id, previous_count)?;
            transaction.commit().map_err(db_error)?;
            return Ok(ids);
        }
        transaction
            .execute(
                "INSERT INTO imports(source, pgn, game_count) VALUES (?1, ?2, ?3)",
                params![source, pgn, game_count as i64],
            )
            .map_err(db_error)?;
        let import_id = transaction.last_insert_rowid();
        let mut ids = Vec::with_capacity(game_count);
        for ordinal in 0..game_count {
            transaction
                .execute(
                    "INSERT INTO games(import_id, ordinal) VALUES (?1, ?2)",
                    params![import_id, ordinal as i64],
                )
                .map_err(db_error)?;
            ids.push(transaction.last_insert_rowid());
        }
        transaction.commit().map_err(db_error)?;
        Ok(ids)
    }

    pub fn list_imports(&self) -> Result<Vec<StoredImport>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT id, source, pgn, game_count FROM imports ORDER BY id")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    StoredImport {
                        id: row.get(0)?,
                        source: row.get(1)?,
                        pgn: row.get(2)?,
                        game_ids: Vec::new(),
                    },
                    bounded_usize(row, 3, 1, MAX_GAMES)?,
                ))
            })
            .map_err(db_error)?;
        let records = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
        let mut imports = Vec::with_capacity(records.len());
        for (mut import, expected_count) in records {
            import.game_ids = game_ids(&self.connection, import.id, expected_count)?;
            imports.push(import);
        }
        Ok(imports)
    }

    pub fn save_analysis(
        &mut self,
        game_id: i64,
        signature: &str,
        ply: usize,
        payload: &str,
    ) -> Result<(), String> {
        validate_ply(ply)?;
        validate_text(payload, "Analysis payload")?;
        let transaction = self.connection.transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO analyses(game_id, signature, ply, payload) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(game_id, signature, ply) DO UPDATE SET payload = excluded.payload",
                params![game_id, signature, ply as i64, payload],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }

    pub fn load_analyses(
        &self,
        game_id: i64,
        signature: &str,
    ) -> Result<Vec<(usize, String)>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT ply, payload FROM analyses WHERE game_id = ?1 AND signature = ?2 ORDER BY ply",
            )
            .map_err(db_error)?;
        statement
            .query_map(params![game_id, signature], |row| {
                Ok((bounded_usize(row, 0, 0, MAX_PLY)?, row.get(1)?))
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)
    }

    pub fn save_session(&mut self, game_id: Option<i64>, ply: usize) -> Result<(), String> {
        validate_ply(ply)?;
        let transaction = self.connection.transaction().map_err(db_error)?;
        if let Some(game_id) = game_id {
            transaction
                .execute(
                    "INSERT INTO active_session(singleton, game_id, ply) VALUES (1, ?1, ?2)
                     ON CONFLICT(singleton) DO UPDATE SET game_id = excluded.game_id, ply = excluded.ply",
                    params![game_id, ply as i64],
                )
                .map_err(db_error)?;
        } else {
            transaction
                .execute("DELETE FROM active_session WHERE singleton = 1", [])
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)
    }

    pub fn session(&self) -> Result<Option<(i64, usize)>, String> {
        self.connection
            .query_row(
                "SELECT game_id, ply FROM active_session WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, bounded_usize(row, 1, 0, MAX_PLY)?)),
            )
            .optional()
            .map_err(db_error)
    }
}

fn bounded_usize(
    row: &rusqlite::Row<'_>,
    column: usize,
    minimum: usize,
    maximum: usize,
) -> rusqlite::Result<usize> {
    let value: i64 = row.get(column)?;
    usize::try_from(value)
        .ok()
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Stored integer {value} is outside {minimum}..={maximum}"),
                )),
            )
        })
}

fn game_ids(
    connection: &Connection,
    import_id: i64,
    expected_count: usize,
) -> Result<Vec<i64>, String> {
    let mut statement = connection
        .prepare("SELECT id, ordinal FROM games WHERE import_id = ?1 ORDER BY ordinal")
        .map_err(db_error)?;
    let rows = statement
        .query_map([import_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                bounded_usize(row, 1, 0, MAX_GAMES - 1)?,
            ))
        })
        .map_err(db_error)?;
    let mut ids = Vec::with_capacity(expected_count);
    for row in rows {
        let (id, ordinal) = row.map_err(db_error)?;
        if ordinal != ids.len() {
            return Err(format!(
                "Library import {import_id} has noncontiguous game ordinals"
            ));
        }
        ids.push(id);
    }
    if ids.len() != expected_count {
        return Err(format!(
            "Library import {import_id} has {} games, expected {expected_count}",
            ids.len()
        ));
    }
    Ok(ids)
}

fn validate_version(version: i64) -> Result<(), String> {
    if version > SCHEMA_VERSION {
        return Err(format!(
            "Library schema version {version} is newer than supported version {SCHEMA_VERSION}"
        ));
    }
    if version != 0 && version != SCHEMA_VERSION {
        return Err(format!("Unsupported library schema version {version}"));
    }
    Ok(())
}

fn validate_schema(connection: &Connection) -> Result<(), String> {
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(db_error)?;
    if integrity != "ok" {
        return Err(format!("Library integrity check failed: {integrity}"));
    }
    for query in [
        "SELECT id, source, pgn, game_count FROM imports LIMIT 0",
        "SELECT id, import_id, ordinal FROM games LIMIT 0",
        "SELECT game_id, signature, ply, payload FROM analyses LIMIT 0",
        "SELECT singleton, game_id, ply FROM active_session LIMIT 0",
    ] {
        connection.prepare(query).map_err(db_error)?;
    }
    let foreign_key_violation: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(db_error)?;
    if let Some(table) = foreign_key_violation {
        return Err(format!("Library has invalid foreign keys in {table}"));
    }
    Ok(())
}

fn validate_text(value: &str, label: &str) -> Result<(), String> {
    if value.len() > MAX_TEXT_BYTES {
        return Err(format!("{label} exceeds the 8 MiB limit"));
    }
    Ok(())
}

fn validate_ply(ply: usize) -> Result<(), String> {
    if ply > MAX_PLY {
        return Err(format!("Ply must be in 0..={MAX_PLY}"));
    }
    Ok(())
}

fn db_error(error: rusqlite::Error) -> String {
    format!("Library database error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_test_library() -> (tempfile::TempDir, Library) {
        let directory = tempfile::tempdir().unwrap();
        let library = Library::open(&directory.path().join("library.sqlite3")).unwrap();
        (directory, library)
    }

    #[test]
    fn stored_integer_conversion_rejects_negative_and_out_of_range_values() {
        let (_directory, library) = open_test_library();
        for value in [-1, MAX_PLY as i64 + 1, i64::MAX] {
            assert!(
                library
                    .connection
                    .query_row("SELECT ?1", [value], |row| bounded_usize(
                        row, 0, 0, MAX_PLY
                    ))
                    .is_err()
            );
        }
        assert!(
            library
                .connection
                .query_row("SELECT 0", [], |row| bounded_usize(row, 0, 1, MAX_GAMES))
                .is_err()
        );
        assert_eq!(
            library
                .connection
                .query_row("SELECT 2048", [], |row| bounded_usize(row, 0, 0, MAX_PLY))
                .unwrap(),
            MAX_PLY
        );
    }

    #[test]
    fn imports_analysis_and_session_persist_after_reopening() {
        let (directory, mut library) = open_test_library();
        let ids = library.import("games.pgn", "PGN contents", 2).unwrap();
        library
            .save_analysis(ids[1], "engine-config", 9, "result")
            .unwrap();
        library.save_session(Some(ids[1]), 9).unwrap();
        drop(library);
        let library = Library::open(&directory.path().join("library.sqlite3")).unwrap();
        let imports = library.list_imports().unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "games.pgn");
        assert_eq!(imports[0].pgn, "PGN contents");
        assert_eq!(imports[0].game_ids, ids);
        assert_eq!(
            library.load_analyses(ids[1], "engine-config").unwrap(),
            vec![(9, "result".into())]
        );
        assert_eq!(library.session().unwrap(), Some((ids[1], 9)));
    }

    #[test]
    fn duplicate_pgn_reuses_ids_even_with_another_source() {
        let (_directory, mut library) = open_test_library();
        let ids = library.import("first.pgn", "same contents", 2).unwrap();
        assert_eq!(
            library.import("renamed.pgn", "same contents", 2).unwrap(),
            ids
        );
        assert!(library.import("first.pgn", "same contents", 1).is_err());
        assert_eq!(library.list_imports().unwrap().len(), 1);
        assert_eq!(library.list_imports().unwrap()[0].source, "first.pgn");
    }

    #[test]
    fn missing_or_noncontiguous_games_are_reported_as_corrupt_imports() {
        for remove_last_game in [true, false] {
            let (_directory, mut library) = open_test_library();
            let ids = library.import("games.pgn", "contents", 2).unwrap();
            if remove_last_game {
                library
                    .connection
                    .execute("DELETE FROM games WHERE id = ?1", [ids[1]])
                    .unwrap();
            } else {
                library
                    .connection
                    .execute("UPDATE games SET ordinal = 2 WHERE id = ?1", [ids[1]])
                    .unwrap();
            }
            assert!(library.list_imports().is_err());
            assert!(library.import("games.pgn", "contents", 2).is_err());
        }
    }

    #[test]
    fn failed_game_insert_rolls_back_the_entire_import() {
        let (_directory, mut library) = open_test_library();
        library
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_second_game BEFORE INSERT ON games WHEN NEW.ordinal = 1
             BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
            )
            .unwrap();
        assert!(library.import("games.pgn", "contents", 2).is_err());
        assert!(library.list_imports().unwrap().is_empty());
        let count: i64 = library
            .connection
            .query_row("SELECT count(*) FROM games", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn analysis_is_separate_per_game_configuration_and_ply_and_upserts() {
        let (_directory, mut library) = open_test_library();
        let ids = library.import("games.pgn", "contents", 2).unwrap();
        library.save_analysis(ids[0], "a", 5, "old").unwrap();
        library.save_analysis(ids[0], "a", 1, "first").unwrap();
        library.save_analysis(ids[0], "a", 5, "new").unwrap();
        library
            .save_analysis(ids[0], "b", 5, "other config")
            .unwrap();
        library.save_analysis(ids[1], "a", 5, "other game").unwrap();
        assert_eq!(
            library.load_analyses(ids[0], "a").unwrap(),
            vec![(1, "first".into()), (5, "new".into())]
        );
        assert_eq!(
            library.load_analyses(ids[0], "b").unwrap(),
            vec![(5, "other config".into())]
        );
        assert_eq!(
            library.load_analyses(ids[1], "a").unwrap(),
            vec![(5, "other game".into())]
        );
        assert!(library.load_analyses(ids[1], "missing").unwrap().is_empty());
    }

    #[test]
    fn session_can_be_replaced_and_cleared_and_rejects_unknown_games() {
        let (_directory, mut library) = open_test_library();
        let ids = library.import("games.pgn", "contents", 2).unwrap();
        assert_eq!(library.session().unwrap(), None);
        library.save_session(Some(ids[0]), 0).unwrap();
        library.save_session(Some(ids[1]), MAX_PLY).unwrap();
        assert!(library.save_session(Some(i64::MAX), 3).is_err());
        assert_eq!(library.session().unwrap(), Some((ids[1], MAX_PLY)));
        library.save_session(None, 0).unwrap();
        assert_eq!(library.session().unwrap(), None);
        assert!(
            library
                .save_analysis(i64::MAX, "config", 0, "payload")
                .is_err()
        );
    }

    #[test]
    fn limits_reject_input_without_partial_writes() {
        let (_directory, mut library) = open_test_library();
        assert!(library.import("test", "contents", 0).is_err());
        assert!(library.import("test", "contents", MAX_GAMES + 1).is_err());
        let oversized = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(library.import("test", &oversized, 1).is_err());
        assert!(library.list_imports().unwrap().is_empty());
        let ids = library.import("test", "contents", MAX_GAMES).unwrap();
        assert_eq!(ids.len(), MAX_GAMES);
        assert!(
            library
                .save_analysis(ids[0], "config", 0, &oversized)
                .is_err()
        );
        assert!(
            library
                .save_analysis(ids[0], "config", MAX_PLY + 1, "payload")
                .is_err()
        );
        assert!(library.save_session(Some(ids[0]), MAX_PLY + 1).is_err());
        assert!(library.load_analyses(ids[0], "config").unwrap().is_empty());
        assert_eq!(library.session().unwrap(), None);
    }

    #[test]
    fn newer_schema_is_rejected_without_modifying_the_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("future.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE future(value TEXT); INSERT INTO future VALUES ('keep me'); PRAGMA user_version = 2;").unwrap();
        drop(connection);
        let before = std::fs::read(&path).unwrap();
        assert!(Library::open(&path).err().unwrap().contains("newer"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn concurrent_first_opens_share_one_initialized_library() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("concurrent.sqlite3");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(9));
        let mut threads = Vec::new();
        for _ in 0..8 {
            let path = path.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                let mut library = Library::open(&path)?;
                library.import("same.pgn", "same contents", 2)
            }));
        }
        barrier.wait();
        let ids = threads
            .into_iter()
            .map(|thread| thread.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert!(ids.iter().all(|game_ids| game_ids == &ids[0]));
        let library = Library::open(&path).unwrap();
        assert_eq!(library.list_imports().unwrap().len(), 1);
    }

    #[test]
    fn corrupt_and_unrecognized_databases_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrupt.sqlite3");
        std::fs::write(&path, b"not a sqlite database").unwrap();
        assert!(Library::open(&path).is_err());
        let path = directory.path().join("unrecognized.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE valuable(value TEXT);")
            .unwrap();
        drop(connection);
        let before = std::fs::read(&path).unwrap();
        assert!(Library::open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let path = directory.path().join("incomplete.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 1;")
            .unwrap();
        drop(connection);
        assert!(Library::open(&path).is_err());
    }
}
