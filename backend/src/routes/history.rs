use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct HistoryQuery {
    /// Unix timestamp – start of window.
    pub from: i64,
    /// Unix timestamp – end of window.
    pub to: i64,
    /// Optional: filter by MMSI.
    pub mmsi: Option<i64>,
}

#[derive(Serialize)]
pub struct HistoryPoint {
    pub mmsi: i64,
    pub lat: f64,
    pub lon: f64,
    pub course: f64,
    pub speed: f64,
    pub heading: f64,
    pub ship_name: String,
    pub ship_type: i64,
    pub recorded_at: i64,
}

/// Return ship history positions as a flat JSON array.
pub async fn get_ship_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HistoryQuery>,
) -> Json<Vec<HistoryPoint>> {
    let rows = state.cache_db.run(move |conn| {
        let rows: Vec<HistoryPoint> = if let Some(mmsi) = q.mmsi {
            let mut stmt = conn.prepare(
                "SELECT mmsi, lat, lon, course, speed, heading, ship_name, ship_type, recorded_at \
                 FROM ship_history WHERE recorded_at BETWEEN ?1 AND ?2 AND mmsi = ?3 \
                 ORDER BY recorded_at"
            )?;
            let result = stmt.query_map(rusqlite::params![q.from, q.to, mmsi], |row| {
                Ok(HistoryPoint {
                    mmsi: row.get(0)?,
                    lat: row.get(1)?,
                    lon: row.get(2)?,
                    course: row.get(3)?,
                    speed: row.get(4)?,
                    heading: row.get(5)?,
                    ship_name: row.get(6)?,
                    ship_type: row.get(7)?,
                    recorded_at: row.get(8)?,
                })
            })?.filter_map(|r| r.ok()).collect();
            result
        } else {
            let mut stmt = conn.prepare(
                "SELECT mmsi, lat, lon, course, speed, heading, ship_name, ship_type, recorded_at \
                 FROM ship_history WHERE recorded_at BETWEEN ?1 AND ?2 \
                 ORDER BY recorded_at LIMIT 50000"
            )?;
            let result = stmt.query_map(rusqlite::params![q.from, q.to], |row| {
                Ok(HistoryPoint {
                    mmsi: row.get(0)?,
                    lat: row.get(1)?,
                    lon: row.get(2)?,
                    course: row.get(3)?,
                    speed: row.get(4)?,
                    heading: row.get(5)?,
                    ship_name: row.get(6)?,
                    ship_type: row.get(7)?,
                    recorded_at: row.get(8)?,
                })
            })?.filter_map(|r| r.ok()).collect();
            result
        };
        Ok(rows)
    }).await.unwrap_or_default();
    Json(rows)
}

/// Return distinct timestamps available in ship_history (for time slider ticks).
#[derive(Serialize)]
pub struct HistoryTimestamps {
    pub timestamps: Vec<i64>,
    pub total_snapshots: i64,
}

pub async fn get_history_timestamps(
    State(state): State<Arc<AppState>>,
) -> Json<HistoryTimestamps> {
    let result = state.cache_db.run(|conn| {
        let total: i64 = conn
            .query_row("SELECT COUNT(DISTINCT recorded_at) FROM ship_history", [], |row| row.get(0))
            .unwrap_or(0);
        let mut stmt = conn.prepare(
            "SELECT DISTINCT recorded_at FROM ship_history ORDER BY recorded_at DESC LIMIT 500"
        )?;
        let ts: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(HistoryTimestamps { timestamps: ts, total_snapshots: total })
    }).await.unwrap_or(HistoryTimestamps { timestamps: vec![], total_snapshots: 0 });
    Json(result)
}
