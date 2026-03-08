use axum::{extract::State, Json};
use rusqlite::Connection;
use std::sync::Arc;

use crate::state::AppState;

/// Load airports from static.db into a GeoJSON Value (called once at startup)
pub fn load_airports(conn: &Connection) -> serde_json::Value {
    let mut features = Vec::new();

    if let Ok(mut stmt) =
        conn.prepare("SELECT icao, name, city, country, lat, lon, elevation_ft FROM airports")
    {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "type": "Feature",
                "geometry": {
                    "type": "Point",
                    "coordinates": [row.get::<_, f64>(5)?, row.get::<_, f64>(4)?]
                },
                "properties": {
                    "icao": row.get::<_, Option<String>>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "city": row.get::<_, Option<String>>(2)?,
                    "country": row.get::<_, Option<String>>(3)?,
                    "elevation_ft": row.get::<_, Option<f64>>(6)?,
                }
            }))
        }) {
            features = rows.filter_map(|r| r.ok()).collect();
        }
    }

    tracing::info!("Loaded {} airports into memory", features.len());
    serde_json::json!({
        "type": "FeatureCollection",
        "features": features,
    })
}

/// Load seaports from static.db into a GeoJSON Value (called once at startup)
pub fn load_seaports(conn: &Connection) -> serde_json::Value {
    let mut features = Vec::new();

    if let Ok(mut stmt) = conn.prepare("SELECT locode, name, country, lat, lon FROM seaports") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "type": "Feature",
                "geometry": {
                    "type": "Point",
                    "coordinates": [row.get::<_, f64>(4)?, row.get::<_, f64>(3)?]
                },
                "properties": {
                    "locode": row.get::<_, Option<String>>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "country": row.get::<_, Option<String>>(2)?,
                }
            }))
        }) {
            features = rows.filter_map(|r| r.ok()).collect();
        }
    }

    tracing::info!("Loaded {} seaports into memory", features.len());
    serde_json::json!({
        "type": "FeatureCollection",
        "features": features,
    })
}

pub async fn get_airports(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(state.airports_geojson.clone())
}

pub async fn get_seaports(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(state.seaports_geojson.clone())
}
