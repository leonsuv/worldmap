use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct DbPool {
    conn: Arc<Mutex<Connection>>,
}

impl DbPool {
    pub fn open_cache(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .with_context(|| format!("opening cache db at {:?}", path.as_ref()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_cache (
                key        TEXT PRIMARY KEY,
                body       BLOB NOT NULL,
                fetched_at INTEGER NOT NULL,
                ttl_secs   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cache_ttl ON api_cache(fetched_at);

            CREATE TABLE IF NOT EXISTS ships (
                mmsi       INTEGER PRIMARY KEY,
                lat        REAL NOT NULL,
                lon        REAL NOT NULL,
                course     REAL,
                speed      REAL,
                heading    REAL,
                ship_name  TEXT NOT NULL DEFAULT '',
                ship_type  INTEGER,
                updated_at INTEGER NOT NULL
            );

            -- Historical ship position snapshots (every 5 min)
            CREATE TABLE IF NOT EXISTS ship_history (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                mmsi       INTEGER NOT NULL,
                lat        REAL NOT NULL,
                lon        REAL NOT NULL,
                course     REAL,
                speed      REAL,
                heading    REAL,
                ship_name  TEXT NOT NULL DEFAULT '',
                ship_type  INTEGER,
                recorded_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ship_history_time ON ship_history(recorded_at);
            CREATE INDEX IF NOT EXISTS idx_ship_history_mmsi ON ship_history(mmsi, recorded_at);

            -- Watchlist items (ports, areas, vessels, assets)
            CREATE TABLE IF NOT EXISTS watchlist (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                wtype      TEXT NOT NULL, -- 'vessel','port','area','reactor','pipeline'
                name       TEXT NOT NULL,
                params     TEXT NOT NULL DEFAULT '{}', -- JSON: {mmsi, lat, lon, radius_km, ...}
                created_at INTEGER NOT NULL
            );

            -- Events / Incidents
            CREATE TABLE IF NOT EXISTS events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                event_type  TEXT NOT NULL, -- 'storm','outage','closure','geopolitical','custom'
                lat         REAL NOT NULL,
                lon         REAL NOT NULL,
                radius_km   REAL NOT NULL DEFAULT 50,
                description TEXT NOT NULL DEFAULT '',
                started_at  INTEGER NOT NULL,
                ended_at    INTEGER,
                active      INTEGER NOT NULL DEFAULT 1
            );

            -- Alerts (generated when watchlist item intersects event)
            CREATE TABLE IF NOT EXISTS alerts (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id        INTEGER,
                title           TEXT NOT NULL,
                message         TEXT NOT NULL,
                severity        TEXT NOT NULL DEFAULT 'warning', -- 'info','warning','critical'
                acknowledged    INTEGER NOT NULL DEFAULT 0,
                created_at      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_alerts_ack ON alerts(acknowledged, created_at);
            ",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_static(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path.as_ref(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("opening static db at {:?}", path.as_ref()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn cache_get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        let mut stmt = conn.prepare_cached(
            "SELECT body FROM api_cache WHERE key = ?1 AND (fetched_at + ttl_secs) > ?2",
        )?;
        let result = stmt.query_row(rusqlite::params![key, now], |row| {
            let blob: Vec<u8> = row.get(0)?;
            Ok(String::from_utf8_lossy(&blob).into_owned())
        });
        match result {
            Ok(body) => Ok(Some(body)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get cached value even if expired (for fallback on upstream errors).
    pub fn cache_get_stale(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT body FROM api_cache WHERE key = ?1",
        )?;
        let result = stmt.query_row(rusqlite::params![key], |row| {
            let blob: Vec<u8> = row.get(0)?;
            Ok(String::from_utf8_lossy(&blob).into_owned())
        });
        match result {
            Ok(body) => Ok(Some(body)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn cache_set(&self, key: &str, body: &str, ttl_secs: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT OR REPLACE INTO api_cache (key, body, fetched_at, ttl_secs) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key, body.as_bytes(), now, ttl_secs],
        )?;
        Ok(())
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }

    /// Load all ships updated within the last `max_age_secs` seconds.
    pub fn load_ships(&self, max_age_secs: i64) -> Result<Vec<(u64, f64, f64, Option<f64>, Option<f64>, Option<f64>, String, Option<u32>, i64)>> {
        let conn = self.conn.lock().unwrap();
        let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
        let mut stmt = conn.prepare(
            "SELECT mmsi, lat, lon, course, speed, heading, ship_name, ship_type, updated_at
             FROM ships WHERE updated_at > ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![cutoff], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get(1)?, row.get(2)?,
                row.get(3)?, row.get(4)?, row.get(5)?,
                row.get(6)?, row.get(7)?,
                row.get(8)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Persist a batch of ship positions (upsert).
    pub fn save_ships(&self, ships: &[(u64, f64, f64, Option<f64>, Option<f64>, Option<f64>, &str, Option<u32>, i64)]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "INSERT OR REPLACE INTO ships (mmsi, lat, lon, course, speed, heading, ship_name, ship_type, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        )?;
        for s in ships {
            stmt.execute(rusqlite::params![s.0 as i64, s.1, s.2, s.3, s.4, s.5, s.6, s.7, s.8])?;
        }
        Ok(())
    }

    /// Append a batch of ship positions into the history table.
    pub fn save_ship_history(&self, ships: &[(u64, f64, f64, Option<f64>, Option<f64>, Option<f64>, &str, Option<u32>, i64)]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "INSERT INTO ship_history (mmsi, lat, lon, course, speed, heading, ship_name, ship_type, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        )?;
        for s in ships {
            stmt.execute(rusqlite::params![s.0 as i64, s.1, s.2, s.3, s.4, s.5, s.6, s.7, s.8])?;
        }
        Ok(())
    }

    /// Load ship history for a time range, returning GeoJSON-ready rows.
    pub fn load_ship_history(&self, from: i64, to: i64) -> Result<Vec<(u64, f64, f64, Option<f64>, Option<f64>, Option<f64>, String, Option<u32>, i64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT mmsi, lat, lon, course, speed, heading, ship_name, ship_type, recorded_at
             FROM ship_history WHERE recorded_at BETWEEN ?1 AND ?2
             ORDER BY recorded_at ASC"
        )?;
        let rows = stmt.query_map(rusqlite::params![from, to], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get(1)?, row.get(2)?,
                row.get(3)?, row.get(4)?, row.get(5)?,
                row.get(6)?, row.get(7)?,
                row.get(8)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Prune old history entries (keep last N days).
    pub fn prune_ship_history(&self, max_age_secs: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
        let count = conn.execute("DELETE FROM ship_history WHERE recorded_at < ?1", rusqlite::params![cutoff])?;
        Ok(count)
    }
}
