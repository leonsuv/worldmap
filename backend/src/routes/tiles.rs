use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::state::AppState;

#[derive(Clone)]
pub struct TileIndex {
    sources: HashMap<String, Arc<Mutex<Connection>>>,
}

impl TileIndex {
    pub fn load_from_dir(dir: &std::path::Path) -> anyhow::Result<Self> {
        let mut sources = HashMap::new();

        if !dir.exists() {
            tracing::warn!("Tiles directory {:?} does not exist, no tiles loaded", dir);
            return Ok(Self { sources });
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("mbtiles") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let conn = Connection::open_with_flags(
                    &path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                tracing::info!("Loaded tile source: {} from {:?}", name, path);
                sources.insert(name, Arc::new(Mutex::new(conn)));
            }
        }

        Ok(Self { sources })
    }

    pub fn source_names(&self) -> Vec<&str> {
        self.sources.keys().map(|s| s.as_str()).collect()
    }
}

#[derive(Serialize)]
pub struct TileJson {
    tilejson: &'static str,
    name: String,
    tiles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    minzoom: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maxzoom: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

pub async fn tilejson(
    State(state): State<Arc<AppState>>,
    Path(source): Path<String>,
) -> Result<Json<TileJson>, StatusCode> {
    let conn = state
        .tile_index
        .sources
        .get(&source)
        .ok_or(StatusCode::NOT_FOUND)?
        .clone();

    let src = source.clone();
    let (minzoom, maxzoom, format) = tokio::task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        let mut minzoom = None;
        let mut maxzoom = None;
        let mut format = None;

        if let Ok(mut stmt) = conn.prepare("SELECT name, value FROM metadata") {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    match row.0.as_str() {
                        "minzoom" => minzoom = row.1.parse().ok(),
                        "maxzoom" => maxzoom = row.1.parse().ok(),
                        "format" => format = Some(row.1),
                        _ => {}
                    }
                }
            }
        }
        (minzoom, maxzoom, format)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TileJson {
        tilejson: "3.0.0",
        name: src.clone(),
        tiles: vec![format!("/tiles/{src}/{{z}}/{{x}}/{{y}}")],
        minzoom,
        maxzoom,
        format,
    }))
}

pub async fn get_tile(
    State(state): State<Arc<AppState>>,
    Path((source, z, x, y)): Path<(String, u32, u32, u32)>,
) -> impl IntoResponse {
    let conn = match state.tile_index.sources.get(&source) {
        Some(c) => c.clone(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    // MBTiles uses TMS Y flipping
    let tms_y = (1u32 << z) - 1 - y;

    let result: Result<Vec<u8>, _> = tokio::task::spawn_blocking(move || {
        let conn = conn.lock().unwrap();
        conn.query_row(
            "SELECT tile_data FROM tiles WHERE zoom_level=?1 AND tile_column=?2 AND tile_row=?3",
            rusqlite::params![z, x, tms_y],
            |row| row.get(0),
        )
    })
    .await
    .unwrap_or(Err(rusqlite::Error::QueryReturnedNoRows));

    match result {
        Ok(tile_data) => {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "application/x-protobuf".parse().unwrap(),
            );
            // MBTiles store gzip-compressed protobuf data
            headers.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
            headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=86400".parse().unwrap(),
            );
            (StatusCode::OK, headers, tile_data).into_response()
        }
        // Return 204 for missing tiles — MapLibre treats 404 as errors,
        // but 204 means "no data for this tile" and is handled gracefully.
        Err(_) => StatusCode::NO_CONTENT.into_response(),
    }
}
