use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Serialize)]
pub struct Alert {
    pub id: i64,
    pub event_id: Option<i64>,
    pub title: String,
    pub message: String,
    pub severity: String,
    pub acknowledged: bool,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct AlertsQuery {
    pub unread_only: Option<bool>,
    pub limit: Option<i64>,
}

/// List alerts, most recent first.
pub async fn list_alerts(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AlertsQuery>,
) -> Json<Vec<Alert>> {
    let items = state.cache_db.run(move |conn| {
        let limit = q.limit.unwrap_or(100).min(500);
        let sql = if q.unread_only.unwrap_or(false) {
            format!("SELECT id, event_id, title, message, severity, acknowledged, created_at FROM alerts WHERE acknowledged = 0 ORDER BY created_at DESC LIMIT {limit}")
        } else {
            format!("SELECT id, event_id, title, message, severity, acknowledged, created_at FROM alerts ORDER BY created_at DESC LIMIT {limit}")
        };
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt
            .query_map([], |row| {
                Ok(Alert {
                    id: row.get(0)?,
                    event_id: row.get(1)?,
                    title: row.get(2)?,
                    message: row.get(3)?,
                    severity: row.get(4)?,
                    acknowledged: row.get::<_, i64>(5)? != 0,
                    created_at: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(items)
    }).await.unwrap_or_default();
    Json(items)
}

/// Get count of unacknowledged alerts (for badge).
#[derive(Serialize)]
pub struct AlertCount {
    pub count: i64,
}

pub async fn alert_count(
    State(state): State<Arc<AppState>>,
) -> Json<AlertCount> {
    let count = state.cache_db.run(|conn| {
        Ok(conn.query_row("SELECT COUNT(*) FROM alerts WHERE acknowledged = 0", [], |row| row.get(0)).unwrap_or(0i64))
    }).await.unwrap_or(0);
    Json(AlertCount { count })
}

/// Acknowledge (mark as read) a single alert.
pub async fn acknowledge_alert(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> StatusCode {
    let changed = state.cache_db.run(move |conn| {
        Ok(conn.execute("UPDATE alerts SET acknowledged = 1 WHERE id = ?1", rusqlite::params![id]).unwrap_or(0))
    }).await.unwrap_or(0);
    if changed > 0 { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

/// Acknowledge all alerts.
pub async fn acknowledge_all(
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    let _ = state.cache_db.run(|conn| {
        conn.execute("UPDATE alerts SET acknowledged = 1 WHERE acknowledged = 0", [])?;
        Ok(())
    }).await;
    StatusCode::NO_CONTENT
}
