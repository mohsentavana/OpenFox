pub mod bookmarks;
pub mod forms;
pub mod history;
pub mod metadata;
pub mod migrate;

pub use bookmarks::Bookmark;
pub use forms::{Address, AutofillField, CreditCard, FormStore};
pub use history::HistoryHit;
pub use metadata::{extract, frecency, favicon_candidates, FrecencyEvent, PageMetadata};
pub use migrate::migrate;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// One connection, WAL mode, one schema version applied at open. Migrations
/// run as a single batch; a half-applied schema is a corrupted file, which
/// WAL makes practically impossible.
pub struct Store {
    conn: rusqlite::Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS visits (
    url TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    visit_count INTEGER NOT NULL DEFAULT 0,
    last_visit_ms INTEGER NOT NULL DEFAULT 0,
    blocked_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS visits_last ON visits(last_visit_ms DESC);

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
);
";

impl Store {
    pub fn open(path: &std::path::Path) -> Result<Store, StoreError> {
        let conn = rusqlite::Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn conn(&mut self) -> &mut rusqlite::Connection {
        &mut self.conn
    }

    pub fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_millis() as i64
    }
}
