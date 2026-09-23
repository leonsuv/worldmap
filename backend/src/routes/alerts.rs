//! Alerts raised when watched items fall inside active events.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::events::{load_events, Event};
use super::watchlist::{load_items, WatchlistItem};
use super::{ApiError, ApiResult};
use crate::geo::haversine_km;
use crate::state::AppState;

#[derive(Serialize, Debug, Clone)]
pub struct Alert {
    pub id: i64,
    pub event_id: Option<i64>,
    pub watch_id: Option<i64>,
    pub title: String,
    pub message: String,
    pub severity: String,
    pub acknowledged: bool,
    pub created_at: i64,
    pub distance_km: Option<f64>,
}

/// A watched item's current position and extra radius (for areas).
fn position(state: &AppState, item: &WatchlistItem) -> Option<(f64, f64, f64)> {
    if let Some(mmsi) = item.params.get("mmsi").and_then(Value::as_u64) {
        return state.ais.ships.get(&mmsi).map(|s| (s.lat, s.lon, 0.0));
    }
    let lat = item.params.get("lat")?.as_f64()?;
    let lon = item.params.get("lon")?.as_f64()?;
    Some((lat, lon, item.params.get("radius_km").and_then(Value::as_f64).unwrap_or(0.0)))
}

fn label(wtype: &str) -> &str {
    match wtype {
        "vessel" => "Vessel",
        "port" => "Port",
        "airport" => "Airport",
        "reactor" => "Nuclear plant",
        "area" => "Area",
        "pipeline" => "Pipeline",
        other => other,
    }
}

/// Alerts for every (event, item) pair that intersects, as insert parameters.
pub fn matches(state: &AppState, events: &[Event], items: &[WatchlistItem]) -> Vec<(i64, i64, String, String, &'static str, f64)> {
    let mut out = Vec::new();
    for item in items {
        let Some((lat, lon, extra)) = position(state, item) else { continue };
        for event in events.iter().filter(|e| e.active) {
            let distance = haversine_km(event.lat, event.lon, lat, lon);
            if distance > event.radius_km + extra {
                continue;
            }
            let severity = if distance <= event.radius_km * 0.5 { "critical" } else { "warning" };
            let title = format!("{} in {}", item.name, event.name);
            let message = format!(
                "{} “{}” is {:.0} km from the centre of the {} event “{}” (radius {:.0} km).",
                label(&item.wtype),
                item.name,
                distance,
                event.event_type,
                event.name,
                event.radius_km
            );
            out.push((event.id, item.id, title, message, severity, distance));
        }
    }
    out
}

/// Raise alerts for intersecting pairs, optionally limited to one event or item.
/// Each pair alerts at most once. Returns the number of new alerts.
pub async fn evaluate(state: &Arc<AppState>, event_id: Option<i64>, watch_id: Option<i64>) -> usize {
    let loaded = state.cache_db.run(move |conn| Ok((load_events(conn, true)?, load_items(conn)?))).await;
    let Ok((mut events, mut items)) = loaded else { return 0 };
    if let Some(id) = event_id {
        events.retain(|e| e.id == id);
    }
    if let Some(id) = watch_id {
        items.retain(|i| i.id == id);
    }
    let found = matches(state, &events, &items);
    if found.is_empty() {
        return 0;
    }
    let now = chrono::Utc::now().timestamp();
    let result = state
        .cache_db
        .run(move |conn| {
            let mut inserted = 0;
            let mut stmt = conn.prepare_cached(
                "INSERT OR IGNORE INTO alerts (event_id, watch_id, title, message, severity, created_at, distance_km)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (event, watch, title, message, severity, distance) in found {
                inserted += stmt.execute(rusqlite::params![event, watch, title, message, severity, now, distance])?;
            }
            Ok(inserted)
        })
        .await;
    match result {
        Ok(n) => {
            if n > 0 {
                tracing::info!("Raised {n} new alert(s)");
            }
            n
        }
        Err(e) => {
            tracing::warn!("Alert evaluation failed: {e:#}");
            0
        }
    }
}

#[derive(Deserialize)]
pub struct AlertsQuery {
    pub unread_only: Option<bool>,
    pub limit: Option<i64>,
}

pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<AlertsQuery>) -> ApiResult<Json<Vec<Alert>>> {
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let unread = q.unread_only.unwrap_or(false);
    let alerts = state
        .cache_db
        .run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, event_id, watch_id, title, message, severity, acknowledged, created_at, distance_km
                 FROM alerts WHERE (?1 = 0 OR acknowledged = 0) ORDER BY created_at DESC, id DESC LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![unread as i64, limit], |row| {
                    Ok(Alert {
                        id: row.get(0)?,
                        event_id: row.get(1)?,
                        watch_id: row.get(2)?,
                        title: row.get(3)?,
                        message: row.get(4)?,
                        severity: row.get(5)?,
                        acknowledged: row.get::<_, i64>(6)? != 0,
                        created_at: row.get(7)?,
                        distance_km: row.get(8)?,
                    })
                })?
                .filter_map(Result::ok)
                .collect();
            Ok(rows)
        })
        .await?;
    Ok(Json(alerts))
}

pub async fn count(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let count: i64 =
        state.cache_db.run(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM alerts WHERE acknowledged = 0", [], |r| r.get(0))?)).await?;
    Ok(Json(serde_json::json!({ "count": count })))
}

pub async fn acknowledge(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let changed = state.cache_db.run(move |conn| Ok(conn.execute("UPDATE alerts SET acknowledged = 1 WHERE id = ?1", [id])?)).await?;
    if changed > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Alert not found"))
    }
}

pub async fn acknowledge_all(State(state): State<Arc<AppState>>) -> ApiResult<StatusCode> {
    state.cache_db.run(|conn| Ok(conn.execute("UPDATE alerts SET acknowledged = 1 WHERE acknowledged = 0", [])?)).await?;
    Ok(StatusCode::NO_CONTENT)
}
