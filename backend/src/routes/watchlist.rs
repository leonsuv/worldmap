use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Serialize, Deserialize)]
pub struct WatchlistItem {
    pub id: i64,
    pub wtype: String,
    pub name: String,
    pub params: serde_json::Value,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct CreateWatchlistItem {
    pub wtype: String,
    pub name: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

pub async fn list_watchlist(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<WatchlistItem>> {
    let conn = state.cache_db.conn();
    let mut stmt = conn
        .prepare("SELECT id, wtype, name, params, created_at FROM watchlist ORDER BY created_at DESC")
        .unwrap();
    let items: Vec<WatchlistItem> = stmt
        .query_map([], |row| {
            let params_str: String = row.get(3)?;
            Ok(WatchlistItem {
                id: row.get(0)?,
                wtype: row.get(1)?,
                name: row.get(2)?,
                params: serde_json::from_str(&params_str).unwrap_or(serde_json::Value::Null),
                created_at: row.get(4)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    Json(items)
}

pub async fn create_watchlist_item(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWatchlistItem>,
) -> (StatusCode, Json<WatchlistItem>) {
    let now = chrono::Utc::now().timestamp();
    let params_str = serde_json::to_string(&body.params).unwrap_or_default();
    let conn = state.cache_db.conn();
    conn.execute(
        "INSERT INTO watchlist (wtype, name, params, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![body.wtype, body.name, params_str, now],
    )
    .unwrap();
    let id = conn.last_insert_rowid();
    (
        StatusCode::CREATED,
        Json(WatchlistItem {
            id,
            wtype: body.wtype,
            name: body.name,
            params: body.params,
            created_at: now,
        }),
    )
}

pub async fn delete_watchlist_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> StatusCode {
    let conn = state.cache_db.conn();
    let changed = conn
        .execute("DELETE FROM watchlist WHERE id = ?1", rusqlite::params![id])
        .unwrap_or(0);
    if changed > 0 {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
