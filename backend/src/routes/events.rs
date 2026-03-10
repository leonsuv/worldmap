use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Deserialize)]
pub struct EventQuery {
    pub active_only: Option<bool>,
}

/// List all events (optionally only active ones).
pub async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventQuery>,
) -> Json<Vec<Event>> {
    let conn = state.cache_db.conn();
    let sql = if q.active_only.unwrap_or(false) {
        "SELECT id, name, event_type, lat, lon, radius_km, description, started_at, ended_at, active FROM events WHERE active = 1 ORDER BY started_at DESC"
    } else {
        "SELECT id, name, event_type, lat, lon, radius_km, description, started_at, ended_at, active FROM events ORDER BY started_at DESC"
    };
    let mut stmt = conn.prepare(sql).unwrap();
    let items = stmt
        .query_map([], |row| {
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
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    Json(items)
}

/// Create a new event and auto-generate alerts for affected watchlist items.
pub async fn create_event(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateEvent>,
) -> (StatusCode, Json<Event>) {
    let now = chrono::Utc::now().timestamp();
    let conn = state.cache_db.conn();
    conn.execute(
        "INSERT INTO events (name, event_type, lat, lon, radius_km, description, started_at, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
        rusqlite::params![body.name, body.event_type, body.lat, body.lon, body.radius_km, body.description, now],
    ).unwrap();
    let id = conn.last_insert_rowid();

    let event = Event {
        id,
        name: body.name.clone(),
        event_type: body.event_type.clone(),
        lat: body.lat,
        lon: body.lon,
        radius_km: body.radius_km,
        description: body.description.clone(),
        started_at: now,
        ended_at: None,
        active: true,
    };

    // Auto-generate alerts for watchlist items within the event radius
    generate_alerts_for_event(&state, &event);

    (StatusCode::CREATED, Json(event))
}

/// Close/deactivate an event.
pub async fn close_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> StatusCode {
    let conn = state.cache_db.conn();
    let now = chrono::Utc::now().timestamp();
    let changed = conn
        .execute(
            "UPDATE events SET active = 0, ended_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )
        .unwrap_or(0);
    if changed > 0 { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

/// Delete an event entirely.
pub async fn delete_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> StatusCode {
    let conn = state.cache_db.conn();
    conn.execute("DELETE FROM alerts WHERE event_id = ?1", rusqlite::params![id]).ok();
    let changed = conn.execute("DELETE FROM events WHERE id = ?1", rusqlite::params![id]).unwrap_or(0);
    if changed > 0 { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

/// Affected assets query: find all live ships, flights, airports, seaports, reactors
/// within a given event radius.
#[derive(Deserialize)]
pub struct AffectedQuery {
    pub event_id: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub radius_km: Option<f64>,
}

#[derive(Serialize)]
pub struct AffectedAssets {
    pub ships: Vec<serde_json::Value>,
    pub airports: Vec<serde_json::Value>,
    pub seaports: Vec<serde_json::Value>,
    pub reactors: Vec<serde_json::Value>,
    pub total: usize,
}

pub async fn get_affected(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AffectedQuery>,
) -> Result<Json<AffectedAssets>, StatusCode> {
    let (lat, lon, radius_km) = if let Some(eid) = q.event_id {
        // Look up event
        let conn = state.cache_db.conn();
        let result = conn.query_row(
            "SELECT lat, lon, radius_km FROM events WHERE id = ?1",
            rusqlite::params![eid],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?, row.get::<_, f64>(2)?)),
        );
        match result {
            Ok(r) => r,
            Err(_) => return Err(StatusCode::NOT_FOUND),
        }
    } else {
        match (q.lat, q.lon, q.radius_km) {
            (Some(lat), Some(lon), Some(r)) => (lat, lon, r),
            _ => return Err(StatusCode::BAD_REQUEST),
        }
    };

    let mut ships = Vec::new();
    let mut airports = Vec::new();
    let mut seaports = Vec::new();
    let mut reactors = Vec::new();

    // Check ships in store
    for entry in state.ship_store.iter() {
        let s = entry.value();
        if haversine_km(lat, lon, s.lat, s.lon) <= radius_km {
            ships.push(serde_json::json!({
                "mmsi": s.mmsi, "ship_name": s.ship_name, "ship_type": s.ship_type,
                "lat": s.lat, "lon": s.lon, "speed": s.speed, "course": s.course,
                "destination": s.destination, "imo": s.imo,
            }));
        }
    }

    // Check airports
    if let Some(fc) = state.airports_geojson.get("features").and_then(|v| v.as_array()) {
        for f in fc {
            if let Some(coords) = f.pointer("/geometry/coordinates").and_then(|c| c.as_array()) {
                let alon = coords.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let alat = coords.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                if haversine_km(lat, lon, alat, alon) <= radius_km {
                    if let Some(p) = f.get("properties") {
                        airports.push(p.clone());
                    }
                }
            }
        }
    }

    // Check seaports
    if let Some(fc) = state.seaports_geojson.get("features").and_then(|v| v.as_array()) {
        for f in fc {
            if let Some(coords) = f.pointer("/geometry/coordinates").and_then(|c| c.as_array()) {
                let plon = coords.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let plat = coords.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                if haversine_km(lat, lon, plat, plon) <= radius_km {
                    if let Some(p) = f.get("properties") {
                        seaports.push(p.clone());
                    }
                }
            }
        }
    }

    // Check reactors (from static_db)
    {
        let conn = state.static_db.conn();
        let reactor_rows: Vec<_> = {
            let mut stmt = conn.prepare(
                "SELECT name, country, lat, lon, capacity_mw, status, reactor_type FROM nuclear_reactors"
            ).unwrap();
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            }).unwrap().filter_map(|r| r.ok()).collect()
        };
        for row in reactor_rows {
            if haversine_km(lat, lon, row.2, row.3) <= radius_km {
                reactors.push(serde_json::json!({
                    "name": row.0, "country": row.1,
                    "lat": row.2, "lon": row.3,
                    "capacity_mw": row.4, "status": row.5,
                    "reactor_type": row.6,
                }));
            }
        }
    }

    let total = ships.len() + airports.len() + seaports.len() + reactors.len();
    Ok(Json(AffectedAssets { ships, airports, seaports, reactors, total }))
}

// ─── Helpers ───

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Earth radius in km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    r * 2.0 * a.sqrt().asin()
}

/// Check watchlist items against the event and create alerts.
fn generate_alerts_for_event(state: &AppState, event: &Event) {
    let conn = state.cache_db.conn();
    let mut stmt = match conn.prepare("SELECT id, wtype, name, params FROM watchlist") {
        Ok(s) => s,
        Err(_) => return,
    };
    let items: Vec<(i64, String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (_id, wtype, name, params_str) in items {
        let params: serde_json::Value = serde_json::from_str(&params_str).unwrap_or_default();
        let within = match wtype.as_str() {
            "vessel" => {
                // Check if vessel MMSI is currently in the event radius
                if let Some(mmsi) = params.get("mmsi").and_then(|v| v.as_u64()) {
                    state.ship_store.get(&mmsi).map_or(false, |s| {
                        haversine_km(event.lat, event.lon, s.lat, s.lon) <= event.radius_km
                    })
                } else {
                    false
                }
            }
            "port" | "reactor" | "area" => {
                let plat = params.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let plon = params.get("lon").and_then(|v| v.as_f64()).unwrap_or(0.0);
                haversine_km(event.lat, event.lon, plat, plon) <= event.radius_km
            }
            _ => false,
        };

        if within {
            let title = format!("{} affected by {}", name, event.name);
            let message = format!(
                "{} '{}' is within {:.0}km of event '{}' ({})",
                wtype, name, event.radius_km, event.name, event.event_type
            );
            let now = chrono::Utc::now().timestamp();
            conn.execute(
                "INSERT INTO alerts (event_id, title, message, severity, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![event.id, title, message, "warning", now],
            ).ok();
        }
    }
}
