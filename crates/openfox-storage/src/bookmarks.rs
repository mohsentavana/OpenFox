use crate::Store;

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub id: i64,
    pub parent: i64,
    pub title: String,
    pub url: String,
    pub added_ms: i64,
}

impl Store {
    pub fn add_bookmark(&mut self, url: &str, title: &str, parent: i64) -> Result<i64, rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO bookmarks (parent, title, url, added_ms) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![parent, title, url, Store::now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn remove_bookmark(&mut self, id: i64) -> Result<bool, rusqlite::Error> {
        Ok(self.conn.execute("DELETE FROM bookmarks WHERE id = ?1", rusqlite::params![id])? > 0)
    }

    pub fn bookmarks(&self, parent: Option<i64>) -> Result<Vec<Bookmark>, rusqlite::Error> {
        let sql = match parent {
            Some(_) => "SELECT id, parent, title, url, added_ms FROM bookmarks WHERE parent = ?1 ORDER BY added_ms DESC",
            None => "SELECT id, parent, title, url, added_ms FROM bookmarks ORDER BY added_ms DESC",
        };
        let mut stmt = self.conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<Bookmark> {
            Ok(Bookmark {
                id: row.get(0)?,
                parent: row.get(1)?,
                title: row.get(2)?,
                url: row.get(3)?,
                added_ms: row.get(4)?,
            })
        };
        let bookmarks = match parent {
            Some(parent) => stmt
                .query_map(rusqlite::params![parent], map_row)?
                .collect::<Result<_, _>>()?,
            None => stmt.query_map([], map_row)?.collect::<Result<_, _>>()?,
        };
        Ok(bookmarks)
    }

    /// Favicons are stored per host, one blob each, with an etag so refresh
    /// is conditional. Bookmarks render instantly from local state.
    pub fn put_favicon(&mut self, host: &str, bytes: &[u8], etag: Option<&str>) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO favicons (host, bytes, etag) VALUES (?1, ?2, ?3)
             ON CONFLICT(host) DO UPDATE SET bytes = ?2, etag = ?3",
            rusqlite::params![host, bytes, etag],
        )?;
        Ok(())
    }

    pub fn favicon(&self, host: &str) -> Result<Option<(Vec<u8>, Option<String>)>, rusqlite::Error> {
        let mut stmt = self.conn.prepare("SELECT bytes, etag FROM favicons WHERE host = ?1")?;
        let mut rows = stmt.query(rusqlite::params![host])?;
        match rows.next()? {
            Some(row) => Ok(Some((row.get(0)?, row.get(1)?))),
            None => Ok(None),
        }
    }
}
