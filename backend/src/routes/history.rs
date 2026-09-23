//! Recorded vessel snapshots (every 5 minutes) for replay and vessel tracks.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use super::{ApiError, ApiResult};
use crate::state::AppState;

pub const FIELDS: [&str; 8] = ["mmsi", "lon", "lat", "course", "speed", "heading", "ship_type", "name"];

type Row = (i64, f64, f64, Option<f64>, Option<f64>, Option<f64>, Option<u32>, String);

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?))
}

pub async fn timestamps(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let ts: Vec<i64> = state
        .cache_db
        .run(|conn| {
            let mut stmt = conn.prepare("SELECT DISTINCT recorded_at FROM ship_history ORDER BY recorded_at DESC LIMIT 1000")?;
            let mut ts: Vec<i64> = stmt.query_map([], |r| r.get(0))?.filter_map(Result::ok).collect();
            ts.reverse();
            Ok(ts)
        })
        .await?;
    Ok(Json(serde_json::json!({ "timestamps": ts })))
}

#[derive(Deserialize)]
pub struct SnapshotQuery {
    /// One recorded timestamp (preferred).
    pub at: Option<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

/// GET /api/history/ships?at=TS — vessels as recorded at one snapshot.
pub async fn ships(State(state): State<Arc<AppState>>, Query(q): Query<SnapshotQuery>) -> ApiResult<Json<Value>> {
    let (from, to) = match (q.at, q.from, q.to) {
        (Some(at), ..) => (at, at),
        (None, Some(f), Some(t)) if f <= t && t - f <= 3600 => (f, t),
        _ => return Err(ApiError::bad_request("pass at=<timestamp>, or from/to at most one hour apart")),
    };
    let rows: Vec<Row> = state
        .cache_db
        .run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT mmsi, lon, lat, course, speed, heading, ship_type, ship_name FROM ship_history
                 WHERE recorded_at BETWEEN ?1 AND ?2 ORDER BY recorded_at LIMIT 200000",
            )?;
            let rows = stmt.query_map([from, to], read_row)?.filter_map(Result::ok).collect();
            Ok(rows)
        })
        .await?;
    Ok(Json(serde_json::json!({ "from": from, "to": to, "fields": FIELDS, "rows": rows })))
}

#[derive(Deserialize)]
pub struct TrackQuery {
    pub mmsi: i64,
    pub hours: Option<i64>,
}

/// GET /api/history/track?mmsi=…&hours=24 — recorded positions of one vessel.
pub async fn track(State(state): State<Arc<AppState>>, Query(q): Query<TrackQuery>) -> ApiResult<Json<Value>> {
    let hours = q.hours.unwrap_or(24).clamp(1, 72);
    let since = chrono::Utc::now().timestamp() - hours * 3600;
    let mmsi = q.mmsi;
    let points: Vec<(f64, f64, i64, Option<f64>)> = state
        .cache_db
        .run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT lon, lat, recorded_at, speed FROM ship_history WHERE mmsi = ?1 AND recorded_at >= ?2 ORDER BY recorded_at",
            )?;
            let rows =
                stmt.query_map([mmsi, since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.filter_map(Result::ok).collect();
            Ok(rows)
        })
        .await?;
    Ok(Json(serde_json::json!({ "mmsi": mmsi, "points": points })))
}
