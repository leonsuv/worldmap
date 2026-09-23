//! Events (storms, outages, closures, …) and the assets inside them.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::{ApiError, ApiResult};
use crate::geo::{haversine_km, valid_lat_lon};
use crate::state::AppState;

pub const EVENT_TYPES: [&str; 5] = ["storm", "outage", "closure", "geopolitical", "custom"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub id: i64,
    pub name: String,
    pub event_type: String,
    pub lat: f64,
    pub lon: f64,
    pub radius_km: f64,
    pub description: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub active: bool,
}

#[derive(Deserialize)]
pub struct CreateEvent {
    pub name: String,
    pub event_type: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default = "default_radius")]
    pub radius_km: f64,
    #[serde(default)]
    pub description: String,
}

fn default_radius() -> f64 {
    50.0
}

pub fn validate(e: &CreateEvent) -> Result<(), String> {
    let name = e.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err("name must be 1–120 characters".into());
    }
    if !EVENT_TYPES.contains(&e.event_type.as_str()) {
        return Err(format!("event type must be one of: {}", EVENT_TYPES.join(", ")));
    }
    if !valid_lat_lon(e.lat, e.lon) {
        return Err("latitude must be within ±90 and longitude within ±180".into());
    }
    if !(e.radius_km.is_finite() && e.radius_km > 0.0 && e.radius_km <= 5000.0) {
        return Err("radius must be between 0 and 5,000 km".into());
    }
    if e.description.chars().count() > 2000 {
        return Err("description must be at most 2,000 characters".into());
    }
    Ok(())
}

pub fn load_events(conn: &rusqlite::Connection, active_only: bool) -> anyhow::Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, event_type, lat, lon, radius_km, description, started_at, ended_at, active
         FROM events WHERE (?1 = 0 OR active = 1) ORDER BY active DESC, started_at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map([active_only as i64], |row| {
            Ok(Event {
                id: row.get(0)?,
                name: row.get(1)?,
                event_type: row.get(2)?,
                lat: row.get(3)?,
                lon: row.get(4)?,
                radius_km: row.get(5)?,
                description: row.get(6)?,
                started_at: row.get(7)?,
                ended_at: row.get(8)?,
                active: row.get::<_, i64>(9)? != 0,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

#[derive(Deserialize)]
pub struct EventQuery {
    pub active_only: Option<bool>,
}

pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<EventQuery>) -> ApiResult<Json<Vec<Event>>> {
    let active_only = q.active_only.unwrap_or(false);
    Ok(Json(state.cache_db.run(move |conn| load_events(conn, active_only)).await?))
}

pub async fn create(State(state): State<Arc<AppState>>, Json(body): Json<CreateEvent>) -> ApiResult<(StatusCode, Json<Event>)> {
    validate(&body).map_err(ApiError::bad_request)?;
    let now = chrono::Utc::now().timestamp();
    let event = Event {
        id: 0,
        name: body.name.trim().to_string(),
        event_type: body.event_type,
        lat: body.lat,
        lon: body.lon,
        radius_km: body.radius_km,
        description: body.description.trim().to_string(),
        started_at: now,
        ended_at: None,
        active: true,
    };
    let e = event.clone();
    let id = state
        .cache_db
        .run(move |conn| {
            conn.execute(
                "INSERT INTO events (name, event_type, lat, lon, radius_km, description, started_at, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
                rusqlite::params![e.name, e.event_type, e.lat, e.lon, e.radius_km, e.description, e.started_at],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await?;
    super::alerts::evaluate(&state, Some(id), None).await;
    Ok((StatusCode::CREATED, Json(Event { id, ..event })))
}

pub async fn close(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let now = chrono::Utc::now().timestamp();
    let changed = state
        .cache_db
        .run(move |conn| {
            Ok(conn.execute("UPDATE events SET active = 0, ended_at = ?1 WHERE id = ?2 AND active = 1", rusqlite::params![now, id])?)
        })
        .await?;
    if changed > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("No active event with this id"))
    }
}

pub async fn delete(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let changed = state
        .cache_db
        .run(move |conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute("DELETE FROM alerts WHERE event_id = ?1", [id])?;
            let n = tx.execute("DELETE FROM events WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(n)
        })
        .await?;
    if changed > 0 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Event not found"))
    }
}

#[derive(Deserialize)]
pub struct AffectedQuery {
    pub event_id: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub radius_km: Option<f64>,
}

#[derive(Serialize, Default)]
pub struct Affected {
    pub ships: Vec<Value>,
    pub flights: Vec<Value>,
    pub airports: Vec<Value>,
    pub seaports: Vec<Value>,
    pub reactors: Vec<Value>,
    pub total: usize,
}

/// Everything inside a circle, nearest first within each category.
pub fn affected_assets(state: &AppState, lat: f64, lon: f64, radius_km: f64, flights_rows: Option<&Value>) -> Affected {
    fn sorted(mut v: Vec<(f64, Value)>) -> Vec<Value> {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter()
            .map(|(d, mut item)| {
                item["distance_km"] = ((d * 10.0).round() / 10.0).into();
                item
            })
            .collect()
    }
    let within = |plat: f64, plon: f64| {
        let d = haversine_km(lat, lon, plat, plon);
        (d <= radius_km).then_some(d)
    };
    let ds = state.datasets();
    let ships = state
        .ais
        .ships
        .iter()
        .filter_map(|s| {
            within(s.lat, s.lon).map(|d| {
                (d, serde_json::json!({ "mmsi": s.mmsi, "name": s.name, "ship_type": s.ship_type, "lat": s.lat, "lon": s.lon, "speed": s.speed, "destination": s.destination }))
            })
        })
        .collect();
    let flights = flights_rows
        .and_then(|v| v.get("rows"))
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let (flon, flat) = (r.get(3)?.as_f64()?, r.get(4)?.as_f64()?);
                    within(flat, flon).map(|d| {
                        (d, serde_json::json!({ "icao24": r.get(0), "callsign": r.get(1), "country": r.get(2), "lat": flat, "lon": flon, "altitude": r.get(5) }))
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let airports =
        ds.airports.iter().filter_map(|a| within(a.lat, a.lon).map(|d| (d, serde_json::to_value(a).unwrap_or_default()))).collect();
    let seaports =
        ds.seaports.iter().filter_map(|p| within(p.lat, p.lon).map(|d| (d, serde_json::to_value(p).unwrap_or_default()))).collect();
    let reactors = ds
        .plants
        .iter()
        .filter_map(|p| {
            within(p.lat, p.lon).map(|d| {
                (d, serde_json::json!({ "name": p.name, "country": p.country, "lat": p.lat, "lon": p.lon, "capacity_mw": p.capacity_mw, "status": p.status }))
            })
        })
        .collect();
    let mut out = Affected {
        ships: sorted(ships),
        flights: sorted(flights),
        airports: sorted(airports),
        seaports: sorted(seaports),
        reactors: sorted(reactors),
        total: 0,
    };
    out.total = out.ships.len() + out.flights.len() + out.airports.len() + out.seaports.len() + out.reactors.len();
    out
}

/// Last cached flight rows, if any (never triggers an upstream request).
pub async fn cached_flights(state: &AppState) -> Option<Value> {
    let db = state.cache_db.clone();
    let entry = tokio::task::spawn_blocking(move || db.cache_get("opensky:states:v2")).await.ok()?.ok()??;
    serde_json::from_slice(&entry.body).ok()
}

pub async fn affected(State(state): State<Arc<AppState>>, Query(q): Query<AffectedQuery>) -> ApiResult<Json<Affected>> {
    let (lat, lon, radius) = match (q.event_id, q.lat, q.lon, q.radius_km) {
        (Some(id), ..) => {
            let events = state.cache_db.run(|conn| load_events(conn, false)).await?;
            let e = events.into_iter().find(|e| e.id == id).ok_or_else(|| ApiError::not_found("Event not found"))?;
            (e.lat, e.lon, e.radius_km)
        }
        (None, Some(lat), Some(lon), Some(r)) if valid_lat_lon(lat, lon) && r > 0.0 && r <= 5000.0 => (lat, lon, r),
        _ => return Err(ApiError::bad_request("pass event_id, or lat, lon and radius_km")),
    };
    let flights = cached_flights(&state).await;
    Ok(Json(affected_assets(&state, lat, lon, radius, flights.as_ref())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str, event_type: &str, lat: f64, radius_km: f64) -> CreateEvent {
        CreateEvent { name: name.into(), event_type: event_type.into(), lat, lon: 0.0, radius_km, description: String::new() }
    }

    #[test]
    fn events_are_validated() {
        assert!(validate(&event("Storm", "storm", 10.0, 50.0)).is_ok());
        assert!(validate(&event("", "storm", 10.0, 50.0)).is_err());
        assert!(validate(&event("Storm", "party", 10.0, 50.0)).is_err());
        assert!(validate(&event("Storm", "storm", 91.0, 50.0)).is_err());
        assert!(validate(&event("Storm", "storm", 10.0, 0.0)).is_err());
        assert!(validate(&event("Storm", "storm", 10.0, f64::NAN)).is_err());
    }
}
