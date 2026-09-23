//! Watchlist: vessels, ports, airports, plants and areas to monitor.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::{ApiError, ApiResult};
use crate::geo::valid_lat_lon;
use crate::state::AppState;

pub const TYPES: [&str; 6] = ["vessel", "port", "airport", "reactor", "area", "pipeline"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WatchlistItem {
    pub id: i64,
    pub wtype: String,
    pub name: String,
    pub params: Value,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct CreateWatchlistItem {
    pub wtype: String,
    pub name: String,
    #[serde(default)]
    pub params: Value,
}

/// Parameters as stored; older releases double-encoded them as a JSON string.
pub fn parse_params(raw: &str) -> Value {
    let value = serde_json::from_str(raw).unwrap_or(Value::Null);
    if let Value::String(encoded) = value {
        serde_json::from_str(&encoded).unwrap_or(Value::Null)
    } else {
        value
    }
}

/// Validate and normalise parameters for a watchlist type.
pub fn validate(wtype: &str, name: &str, params: &Value) -> Result<Value, String> {
    if !TYPES.contains(&wtype) {
        return Err(format!("type must be one of: {}", TYPES.join(", ")));
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err("name must be 1–120 characters".into());
    }
    if wtype == "vessel" {
        let mmsi = params
            .get("mmsi")
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
            .filter(|m| (100_000_000..=999_999_999).contains(m))
            .ok_or("vessels need a 9-digit MMSI")?;
        return Ok(serde_json::json!({ "mmsi": mmsi }));
    }
    let lat = params.get("lat").and_then(Value::as_f64);
    let lon = params.get("lon").and_then(Value::as_f64);
    let (Some(lat), Some(lon)) = (lat, lon) else { return Err("a latitude and longitude are required".into()) };
    if !valid_lat_lon(lat, lon) {
        return Err("latitude must be within ±90 and longitude within ±180".into());
    }
    let mut out = serde_json::json!({ "lat": lat, "lon": lon });
    if let Some(r) = params.get("radius_km").and_then(Value::as_f64) {
        if !(r.is_finite() && r > 0.0 && r <= 5000.0) {
            return Err("radius must be between 0 and 5,000 km".into());
        }
        out["radius_km"] = r.into();
    }
    Ok(out)
}

pub fn load_items(conn: &rusqlite::Connection) -> anyhow::Result<Vec<WatchlistItem>> {
    let mut stmt = conn.prepare("SELECT id, wtype, name, params, created_at FROM watchlist ORDER BY created_at DESC, id DESC")?;
    let items = stmt
        .query_map([], |row| {
            let params: String = row.get(3)?;
            Ok(WatchlistItem {
                id: row.get(0)?,
                wtype: row.get(1)?,
                name: row.get(2)?,
                params: parse_params(&params),
                created_at: row.get(4)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(items)
}

pub async fn list(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<Value>>> {
    let items = state.cache_db.run(load_items).await?;
    let now = chrono::Utc::now().timestamp();
    let out = items
        .into_iter()
        .map(|item| {
            let mut v = serde_json::to_value(&item).unwrap_or(Value::Null);
            if let Some(mmsi) = item.params.get("mmsi").and_then(Value::as_u64) {
                v["live"] = match state.ais.ships.get(&mmsi) {
                    Some(s) => serde_json::json!({ "lat": s.lat, "lon": s.lon, "speed": s.speed, "age_secs": now - s.timestamp }),
                    None => Value::Null,
                };
            }
            v
        })
        .collect();
    Ok(Json(out))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWatchlistItem>,
) -> ApiResult<(StatusCode, Json<WatchlistItem>)> {
    let params = validate(&body.wtype, &body.name, &body.params).map_err(ApiError::bad_request)?;
    let now = chrono::Utc::now().timestamp();
    let (wtype, name) = (body.wtype.clone(), body.name.trim().to_string());
    let stored = params.to_string();
    let (w, n) = (wtype.clone(), name.clone());
    let id = state
        .cache_db
        .run(move |conn| {
            conn.execute(
                "INSERT INTO watchlist (wtype, name, params, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![w, n, stored, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await?;
    super::alerts::evaluate(&state, None, Some(id)).await;
    Ok((StatusCode::CREATED, Json(WatchlistItem { id, wtype, name, params, created_at: now })))
}

pub async fn delete(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let changed = state.cache_db.run(move |conn| Ok(conn.execute("DELETE FROM watchlist WHERE id = ?1", [id])?)).await?;
    if changed > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Watchlist item not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_and_object_parameters() {
        let object = serde_json::json!({"mmsi": 123});
        assert_eq!(parse_params(&object.to_string()), object);
        assert_eq!(parse_params(&serde_json::to_string(&object.to_string()).unwrap()), object);
        assert!(parse_params("invalid").is_null());
    }

    #[test]
    fn parameters_are_validated_per_type() {
        assert_eq!(validate("vessel", "Ship", &serde_json::json!({"mmsi": "211234567"})).unwrap(), serde_json::json!({"mmsi": 211234567}));
        assert!(validate("vessel", "Ship", &serde_json::json!({"mmsi": 12})).is_err());
        assert!(validate("port", "Hamburg", &serde_json::json!({"lat": 53.5, "lon": 9.9})).is_ok());
        assert!(validate("port", "Hamburg", &serde_json::json!({"lat": 95, "lon": 9.9})).is_err());
        assert!(validate("area", "Box", &serde_json::json!({"lat": 1, "lon": 1, "radius_km": 0})).is_err());
        assert!(validate("spaceship", "X", &serde_json::json!({})).is_err());
        assert!(validate("port", "   ", &serde_json::json!({"lat": 1, "lon": 1})).is_err());
    }
}
