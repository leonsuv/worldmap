//! SQLite access. `cache.db` holds runtime state (API cache, live vessels,
//! history, watchlist, events, alerts); `static.db` holds imported datasets.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::ais::{Ship, ShipKind};

#[derive(Clone)]
pub struct DbPool {
    conn: Arc<Mutex<Connection>>,
}

/// A cached upstream response.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub body: Vec<u8>,
    pub fetched_at: i64,
    pub fresh: bool,
}

fn cache_migrations() -> Migrations<'static> {
    Migrations::new(vec![
        // Baseline: the schema used before migrations existed. Every statement is
        // idempotent so databases created by earlier versions upgrade cleanly.
        M::up(
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
            CREATE TABLE IF NOT EXISTS ship_history (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                mmsi        INTEGER NOT NULL,
                lat         REAL NOT NULL,
                lon         REAL NOT NULL,
                course      REAL,
                speed       REAL,
                heading     REAL,
                ship_name   TEXT NOT NULL DEFAULT '',
                ship_type   INTEGER,
                recorded_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ship_history_time ON ship_history(recorded_at);
            CREATE INDEX IF NOT EXISTS idx_ship_history_mmsi ON ship_history(mmsi, recorded_at);
            CREATE TABLE IF NOT EXISTS watchlist (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                wtype      TEXT NOT NULL,
                name       TEXT NOT NULL,
                params     TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                event_type  TEXT NOT NULL,
                lat         REAL NOT NULL,
                lon         REAL NOT NULL,
                radius_km   REAL NOT NULL DEFAULT 50,
                description TEXT NOT NULL DEFAULT '',
                started_at  INTEGER NOT NULL,
                ended_at    INTEGER,
                active      INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS alerts (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id     INTEGER,
                title        TEXT NOT NULL,
                message      TEXT NOT NULL,
                severity     TEXT NOT NULL DEFAULT 'warning',
                acknowledged INTEGER NOT NULL DEFAULT 0,
                created_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_alerts_ack ON alerts(acknowledged, created_at);",
        ),
        // Full vessel records survive restarts; alerts are tied to the watched item
        // so the monitor never raises the same alert twice.
        M::up(
            "ALTER TABLE ships ADD COLUMN imo INTEGER;
            ALTER TABLE ships ADD COLUMN callsign TEXT;
            ALTER TABLE ships ADD COLUMN destination TEXT;
            ALTER TABLE ships ADD COLUMN eta TEXT;
            ALTER TABLE ships ADD COLUMN draught REAL;
            ALTER TABLE ships ADD COLUMN length INTEGER;
            ALTER TABLE ships ADD COLUMN beam INTEGER;
            ALTER TABLE ships ADD COLUMN nav_status INTEGER;
            ALTER TABLE ships ADD COLUMN kind TEXT NOT NULL DEFAULT 'vessel';
            ALTER TABLE ships ADD COLUMN altitude REAL;
            ALTER TABLE alerts ADD COLUMN watch_id INTEGER;
            ALTER TABLE alerts ADD COLUMN distance_km REAL;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_alerts_event_watch ON alerts(event_id, watch_id);",
        ),
    ])
}

fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;")?;
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    Ok(())
}

impl DbPool {
    /// Run a blocking operation on the blocking thread pool.
    pub async fn run<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
            f(&guard)
        })
        .await
        .map_err(|e| anyhow::anyhow!("database task failed: {e}"))?
    }

    pub fn open_cache(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path != Path::new(":memory:") {
            ensure_parent(path)?;
        }
        let mut conn = Connection::open(path).with_context(|| format!("opening cache db at {}", path.display()))?;
        configure(&conn)?;
        cache_migrations().to_latest(&mut conn).context("migrating cache db")?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn open_static(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        ensure_parent(path)?;
        let conn = Connection::open(path).with_context(|| format!("opening static db at {}", path.display()))?;
        configure(&conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ── API cache ──────────────────────────────────────────────────────────

    pub fn cache_get(&self, key: &str) -> Result<Option<CacheEntry>> {
        let conn = self.conn();
        let now = chrono::Utc::now().timestamp();
        let row = conn
            .prepare_cached("SELECT body, fetched_at, ttl_secs FROM api_cache WHERE key = ?1")?
            .query_row(params![key], |row| {
                let body: Vec<u8> = row.get(0)?;
                let fetched_at: i64 = row.get(1)?;
                let ttl: i64 = row.get(2)?;
                Ok(CacheEntry { body, fetched_at, fresh: fetched_at + ttl > now })
            })
            .optional()?;
        Ok(row)
    }

    pub fn cache_set(&self, key: &str, body: &[u8], ttl_secs: i64) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.conn().execute(
            "INSERT OR REPLACE INTO api_cache (key, body, fetched_at, ttl_secs) VALUES (?1, ?2, ?3, ?4)",
            params![key, body, now, ttl_secs],
        )?;
        Ok(())
    }

    /// Remove cache entries that expired more than `grace_secs` ago.
    pub fn cache_prune(&self, grace_secs: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp() - grace_secs;
        Ok(self.conn().execute("DELETE FROM api_cache WHERE fetched_at + ttl_secs < ?1", params![cutoff])?)
    }

    // ── Vessels ────────────────────────────────────────────────────────────

    pub fn load_ships(&self, max_age_secs: i64) -> Result<Vec<Ship>> {
        let conn = self.conn();
        let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
        let mut stmt = conn.prepare(
            "SELECT mmsi, lat, lon, course, speed, heading, ship_name, ship_type, updated_at,
                    imo, callsign, destination, eta, draught, length, beam, nav_status, kind, altitude
             FROM ships WHERE updated_at > ?1",
        )?;
        let rows = stmt.query_map(params![cutoff], |row| {
            let kind: String = row.get(17)?;
            Ok(Ship {
                mmsi: row.get::<_, i64>(0)? as u64,
                lat: row.get(1)?,
                lon: row.get(2)?,
                course: row.get(3)?,
                speed: row.get(4)?,
                heading: row.get(5)?,
                name: row.get(6)?,
                ship_type: row.get(7)?,
                timestamp: row.get(8)?,
                imo: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                callsign: row.get(10)?,
                destination: row.get(11)?,
                eta: row.get(12)?,
                draught: row.get(13)?,
                length: row.get(14)?,
                beam: row.get(15)?,
                nav_status: row.get(16)?,
                rate_of_turn: None,
                kind: if kind == "sar" { ShipKind::SarAircraft } else { ShipKind::Vessel },
                altitude: row.get(18)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn save_ships(&self, ships: &[Ship]) -> Result<()> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO ships (mmsi, lat, lon, course, speed, heading, ship_name, ship_type, updated_at,
                    imo, callsign, destination, eta, draught, length, beam, nav_status, kind, altitude)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            )?;
            for s in ships {
                stmt.execute(params![
                    s.mmsi as i64,
                    s.lat,
                    s.lon,
                    s.course,
                    s.speed,
                    s.heading,
                    s.name,
                    s.ship_type,
                    s.timestamp,
                    s.imo.map(|v| v as i64),
                    s.callsign,
                    s.destination,
                    s.eta,
                    s.draught,
                    s.length,
                    s.beam,
                    s.nav_status,
                    s.kind.as_str(),
                    s.altitude,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_ships_older_than(&self, max_age_secs: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
        Ok(self.conn().execute("DELETE FROM ships WHERE updated_at < ?1", params![cutoff])?)
    }

    pub fn save_ship_history(&self, ships: &[Ship], recorded_at: i64) -> Result<()> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO ship_history (mmsi, lat, lon, course, speed, heading, ship_name, ship_type, recorded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for s in ships {
                stmt.execute(params![s.mmsi as i64, s.lat, s.lon, s.course, s.speed, s.heading, s.name, s.ship_type, recorded_at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn prune_ship_history(&self, max_age_secs: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
        Ok(self.conn().execute("DELETE FROM ship_history WHERE recorded_at < ?1", params![cutoff])?)
    }
}

/// Column names of a table (empty when the table does not exist).
pub fn table_columns(conn: &Connection, table: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table.replace('"', ""))) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) {
            out.extend(rows.flatten());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(mmsi: u64, name: &str) -> Ship {
        Ship { mmsi, lat: 52.0, lon: 13.0, name: name.into(), timestamp: chrono::Utc::now().timestamp(), ..Ship::default() }
    }

    #[test]
    fn migrations_are_valid_and_upgrade_legacy_databases() {
        assert!(cache_migrations().validate().is_ok());
        let dir = std::env::temp_dir().join(format!("worldmap-legacy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.db");
        let _ = std::fs::remove_file(&path);
        {
            // Schema written by the pre-migration release.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE ships (mmsi INTEGER PRIMARY KEY, lat REAL NOT NULL, lon REAL NOT NULL, course REAL,
                 speed REAL, heading REAL, ship_name TEXT NOT NULL DEFAULT '', ship_type INTEGER, updated_at INTEGER NOT NULL);
                 CREATE TABLE alerts (id INTEGER PRIMARY KEY AUTOINCREMENT, event_id INTEGER, title TEXT NOT NULL,
                 message TEXT NOT NULL, severity TEXT NOT NULL DEFAULT 'warning', acknowledged INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL);
                 INSERT INTO ships (mmsi, lat, lon, ship_name, updated_at) VALUES (1, 1.0, 2.0, 'OLD', strftime('%s','now'));",
            )
            .unwrap();
        }
        let db = DbPool::open_cache(&path).unwrap();
        let ships = db.load_ships(3600).unwrap();
        assert_eq!(ships.len(), 1);
        assert_eq!(ships[0].name, "OLD");
        assert!(table_columns(&db.conn(), "alerts").contains("watch_id"));
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_ship_batch_rolls_back_and_next_batch_succeeds() {
        let db = DbPool::open_cache(":memory:").unwrap();
        db.conn()
            .execute_batch(
                "CREATE TRIGGER reject_bad BEFORE INSERT ON ships WHEN NEW.ship_name = 'bad' BEGIN SELECT RAISE(ABORT, 'x'); END;",
            )
            .unwrap();
        assert!(db.save_ships(&[ship(1, "good"), ship(2, "bad")]).is_err());
        assert_eq!(db.load_ships(3600).unwrap().len(), 0);
        db.save_ships(&[ship(3, "next")]).unwrap();
        assert_eq!(db.load_ships(3600).unwrap().len(), 1);
    }

    #[test]
    fn full_vessel_records_round_trip() {
        let db = DbPool::open_cache(":memory:").unwrap();
        let mut s = ship(211_000_000, "EVER GIVEN");
        s.imo = Some(9_811_000);
        s.callsign = Some("H3RC".into());
        s.length = Some(400);
        s.kind = ShipKind::SarAircraft;
        s.altitude = Some(300.0);
        db.save_ships(&[s]).unwrap();
        let loaded = &db.load_ships(3600).unwrap()[0];
        assert_eq!(loaded.imo, Some(9_811_000));
        assert_eq!(loaded.callsign.as_deref(), Some("H3RC"));
        assert_eq!(loaded.length, Some(400));
        assert_eq!(loaded.kind, ShipKind::SarAircraft);
        assert!(loaded.course.is_none());
    }

    #[test]
    fn cache_entries_report_freshness_and_prune() {
        let db = DbPool::open_cache(":memory:").unwrap();
        db.cache_set("a", b"\x89PNG", 60).unwrap();
        let entry = db.cache_get("a").unwrap().unwrap();
        assert!(entry.fresh);
        assert_eq!(entry.body, b"\x89PNG");
        db.conn().execute("UPDATE api_cache SET fetched_at = fetched_at - 1000", []).unwrap();
        assert!(!db.cache_get("a").unwrap().unwrap().fresh);
        assert_eq!(db.cache_prune(100).unwrap(), 1);
        assert!(db.cache_get("a").unwrap().is_none());
    }
}
