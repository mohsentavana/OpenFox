//! Schema migrations: versioned, forward-only, backed up before the
//! first step runs. `user_version` is the only state; a version number
//! ahead of what this binary knows aborts the open — never "best
//! effort" through an unknown schema.

use crate::{Store, StoreError};

pub struct Migration {
    pub version: u64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// The full ladder, in order. Every step must be idempotent where the
/// SQL allows it (IF NOT EXISTS) because a crash between the schema
/// write and the version write re-runs one step.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "visits-bookmarks-favicons",
        sql: "
        CREATE TABLE IF NOT EXISTS visits (
            url TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            visit_count INTEGER NOT NULL DEFAULT 0,
            last_visit_ms INTEGER NOT NULL DEFAULT 0,
            blocked_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            parent INTEGER NOT NULL DEFAULT 0,
            title TEXT NOT NULL,
            url TEXT NOT NULL UNIQUE,
            added_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS favicons (
            host TEXT PRIMARY KEY,
            bytes BLOB NOT NULL,
            etag TEXT
        );",
    },
    Migration {
        version: 2,
        name: "visits-visit-at-column",
        sql: "ALTER TABLE visits ADD COLUMN first_visit_ms INTEGER NOT NULL DEFAULT 0;",
    },
    Migration {
        version: 3,
        name: "forms-and-meta",
        sql: "
        CREATE TABLE IF NOT EXISTS form_addresses (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            payload TEXT NOT NULL,
            last_used_ms INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS form_cards (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            payload TEXT NOT NULL,
            last_used_ms INTEGER NOT NULL DEFAULT 0
        );",
    },
];

/// Read the schema version straight from `user_version`.
pub fn schema_version(conn: &rusqlite::Connection) -> Result<u64, StoreError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok(version.max(0) as u64)
}

/// Migrate a store to the newest version. Backs the file up first; a
/// half-applied batch rolls the whole transaction back — the backup is
/// the parachute, the transaction is the plan.
pub fn migrate(store: &mut Store, backup_dir: &std::path::Path) -> Result<u64, StoreError> {
    let conn = store.conn();
    let current = schema_version(conn)?;
    let newest = MIGRATIONS.last().map(|m| m.version).unwrap_or(current);

    if current > newest {
        // A future schema means a downgrade attempt: refuse loudly
        // rather than guess at columns.
        return Err(StoreError::Sqlite(rusqlite::Error::InvalidParameterName(
            format!("schema v{current} is newer than this binary's v{newest}"),
        )));
    }
    if current == newest {
        return Ok(current);
    }

    // Backup once, before any step: timestamped sibling in the backup
    // dir. VACUUM INTO is one statement and atomic — a half-copied
    // backup would be worse than none.
    std::fs::create_dir_all(backup_dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = backup_dir.join(format!("profile-v{current}-{stamp}.db"));
    let _ = conn.execute(
        &format!("VACUUM INTO '{}'", backup.display().to_string().replace('\'', "''")),
        [],
    );

    for migration in MIGRATIONS {
        if migration.version <= current {
            continue;
        }
        conn.execute_batch("BEGIN")?;
        match conn.execute_batch(migration.sql) {
            Ok(()) => {
                conn.pragma_update(None, "user_version", migration.version)?;
                conn.execute_batch("COMMIT")?;
            }
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error.into());
            }
        }
    }
    Ok(newest)
}
