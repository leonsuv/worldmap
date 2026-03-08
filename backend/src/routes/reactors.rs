use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct ReactorQuery {
    bbox: Option<String>, // "min_lon,min_lat,max_lon,max_lat"
}

#[derive(Serialize)]
pub struct GeoJsonFeatureCollection {
    r#type: &'static str,
    features: Vec<serde_json::Value>,
}

pub async fn get_reactors(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ReactorQuery>,
) -> Result<Json<GeoJsonFeatureCollection>, axum::http::StatusCode> {
    let conn = state.static_db.conn();

    let features = if let Some(bbox) = q.bbox {
        let parts: Vec<f64> = bbox
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if parts.len() != 4 {
            return Err(axum::http::StatusCode::BAD_REQUEST);
        }
        let (min_lon, min_lat, max_lon, max_lat) = (parts[0], parts[1], parts[2], parts[3]);

        let mut stmt = conn
            .prepare(
                "SELECT r.id, r.name, r.country, r.lat, r.lon, r.capacity_mw, r.status, r.reactor_type
                 FROM nuclear_reactors r
                 JOIN nuclear_reactors_rtree rt ON r.id = rt.id
                 WHERE rt.min_lat <= ?1 AND rt.max_lat >= ?2
                   AND rt.min_lon <= ?3 AND rt.max_lon >= ?4",
            )
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let rows = stmt
            .query_map(rusqlite::params![max_lat, min_lat, max_lon, min_lon], |row| {
                Ok(serde_json::json!({
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [row.get::<_, f64>(4)?, row.get::<_, f64>(3)?]
                    },
                    "properties": {
                        "id": row.get::<_, i64>(0)?,
                        "name": row.get::<_, String>(1)?,
                        "country": row.get::<_, String>(2)?,
                        "capacity_mw": row.get::<_, Option<f64>>(5)?,
                        "status": row.get::<_, Option<String>>(6)?,
                        "reactor_type": row.get::<_, Option<String>>(7)?,
                    }
                }))
            })
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        rows.filter_map(|r| r.ok()).collect()
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, country, lat, lon, capacity_mw, status, reactor_type FROM nuclear_reactors",
            )
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [row.get::<_, f64>(4)?, row.get::<_, f64>(3)?]
                    },
                    "properties": {
                        "id": row.get::<_, i64>(0)?,
                        "name": row.get::<_, String>(1)?,
                        "country": row.get::<_, String>(2)?,
                        "capacity_mw": row.get::<_, Option<f64>>(5)?,
                        "status": row.get::<_, Option<String>>(6)?,
                        "reactor_type": row.get::<_, Option<String>>(7)?,
                    }
                }))
            })
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        rows.filter_map(|r| r.ok()).collect()
    };

    Ok(Json(GeoJsonFeatureCollection {
        r#type: "FeatureCollection",
        features,
    }))
}
