use crate::Store;

#[derive(Debug, Clone)]
pub struct HistoryHit {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit_ms: i64,
    pub score: i64,
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const WEEK_MS: i64 = 7 * DAY_MS;

impl Store {
    /// Records a visit: upsert with count and timestamp. The blocked count
    /// is the page's shield number — history remembers how noisy a site was.
    pub fn record_visit(&mut self, url: &str, title: &str, blocked: u32) -> Result<(), rusqlite::Error> {
        let now = Store::now_ms();
        self.conn.execute(
            "INSERT INTO visits (url, title, visit_count, last_visit_ms, blocked_count)
             VALUES (?1, ?2, 1, ?3, ?4)
             ON CONFLICT(url) DO UPDATE SET
                visit_count = visit_count + 1,
                last_visit_ms = ?3,
                title = CASE WHEN ?2 != '' THEN ?2 ELSE title END,
                blocked_count = blocked_count + ?4",
            rusqlite::params![url, title, now, blocked],
        )?;
        Ok(())
    }

    /// Frecency-ranked search: recent and often-visited rises. The recency
    /// multiplier is computed in Rust where it can be tuned, not buried in
    /// SQL where it cannot.
    pub fn search_history(&self, query: &str, limit: u32) -> Result<Vec<HistoryHit>, rusqlite::Error> {
        let like = format!("%{}%", query.trim());
        let mut stmt = self.conn.prepare(
            "SELECT url, title, visit_count, last_visit_ms FROM visits
             WHERE url LIKE ?1 OR title LIKE ?1
             ORDER BY visit_count DESC, last_visit_ms DESC
             LIMIT 200",
        )?;
        let now = Store::now_ms();
        let mut hits: Vec<HistoryHit> = stmt
            .query_map(rusqlite::params![like], |row| {
                Ok(HistoryHit {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    visit_count: row.get(2)?,
                    last_visit_ms: row.get(3)?,
                    score: 0,
                })
            })?
            .collect::<Result<_, _>>()?;
        for hit in hits.iter_mut() {
            let age = now - hit.last_visit_ms;
            let recency = if age < DAY_MS { 4 } else if age < WEEK_MS { 2 } else { 1 };
            hit.score = hit.visit_count * recency;
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score));
        hits.truncate(limit as usize);
        Ok(hits)
    }

    /// The most-visited sites overall, for the new-tab surface.
    pub fn top_sites(&self, limit: u32) -> Result<Vec<HistoryHit>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT url, title, visit_count, last_visit_ms FROM visits
             ORDER BY visit_count DESC, last_visit_ms DESC LIMIT ?1",
        )?;
        let hits = stmt
            .query_map(rusqlite::params![limit], |row| {
                Ok(HistoryHit {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    visit_count: row.get(2)?,
                    last_visit_ms: row.get(3)?,
                    score: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_upsert_and_rank() {
        let mut store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store.record_visit("https://example.com/a", "A", 3).unwrap();
        store.record_visit("https://example.com/a", "A2", 0).unwrap();
        store.record_visit("https://example.com/b?q=fox", "Fox", 0).unwrap();

        let hits = store.search_history("fox", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/b?q=fox");

        let hits = store.search_history("example.com/a", 10).unwrap();
        assert_eq!(hits[0].visit_count, 2);
        assert_eq!(hits[0].title, "A2");
    }
}
