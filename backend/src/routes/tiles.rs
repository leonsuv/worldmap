//! MBTiles tile server.
//!
//! Sources are discovered in `data/tiles/*.mbtiles` and re-scanned at runtime,
//! so newly built tiles appear without a restart. Each request opens its own
//! read-only connection: nothing keeps the files locked, which lets the tile
//! builder replace them in place (Windows refuses to replace open files).

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crate::live_tiles;
use crate::state::AppState;

#[derive(Clone, Debug, Serialize)]
pub struct TileMeta {
    pub name: String,
    pub format: String,
    pub minzoom: u8,
    pub maxzoom: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f64; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_layers: Option<serde_json::Value>,
}

pub struct TileSource {
    pub id: String,
    pub path: PathBuf,
    modified: Option<SystemTime>,
    pub meta: TileMeta,
}

pub struct TileIndex {
    dir: PathBuf,
    sources: RwLock<HashMap<String, Arc<TileSource>>>,
    /// Every file seen by the last scan (including unusable ones) and its mtime.
    scanned: RwLock<HashMap<String, Option<SystemTime>>>,
}

fn valid_source_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn open_read_only(path: &FsPath) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
}

/// Read MBTiles metadata. Fails when the file is not a usable tileset.
pub fn read_meta(path: &FsPath, id: &str) -> anyhow::Result<TileMeta> {
    let conn = open_read_only(path)?;
    let mut values: HashMap<String, String> = HashMap::new();
    let mut stmt = conn.prepare("SELECT name, value FROM metadata")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.flatten() {
        values.insert(row.0, row.1);
    }
    // Zoom range from metadata, falling back to the stored tiles.
    let (lo, hi): (Option<i64>, Option<i64>) =
        conn.query_row("SELECT MIN(zoom_level), MAX(zoom_level) FROM tiles", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let (Some(lo), Some(hi)) = (lo, hi) else { anyhow::bail!("tileset contains no tiles") };
    let zoom = |key: &str, fallback: i64| values.get(key).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(fallback).clamp(0, 24) as u8;
    let bounds = values.get("bounds").and_then(|b| {
        let v: Vec<f64> = b.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
    });
    let vector_layers =
        values.get("json").and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok()).and_then(|j| j.get("vector_layers").cloned());
    Ok(TileMeta {
        name: values.get("name").cloned().unwrap_or_else(|| id.to_string()),
        format: values.get("format").cloned().unwrap_or_else(|| "pbf".into()),
        minzoom: zoom("minzoom", lo),
        maxzoom: zoom("maxzoom", hi),
        bounds,
        attribution: values.get("attribution").cloned(),
        description: values.get("description").cloned(),
        vector_layers,
    })
}

impl TileIndex {
    pub fn new(dir: PathBuf) -> Self {
        let index = Self { dir, sources: RwLock::new(HashMap::new()), scanned: RwLock::new(HashMap::new()) };
        index.rescan();
        index
    }

    /// Pick up added, changed and removed tilesets. Returns true on change.
    pub fn rescan(&self) -> bool {
        let mut found: HashMap<String, (PathBuf, Option<SystemTime>)> = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("mbtiles") {
                    continue;
                }
                let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
                if !valid_source_id(&id) {
                    tracing::warn!("Ignoring tileset with unsupported name: {}", path.display());
                    continue;
                }
                let modified = entry.metadata().and_then(|m| m.modified()).ok();
                found.insert(id, (path, modified));
            }
        }

        let seen: HashMap<String, Option<SystemTime>> = found.iter().map(|(id, (_, m))| (id.clone(), *m)).collect();
        if *self.scanned.read().unwrap_or_else(|e| e.into_inner()) == seen {
            return false;
        }
        *self.scanned.write().unwrap_or_else(|e| e.into_inner()) = seen;
        let current = self.sources.read().unwrap_or_else(|e| e.into_inner()).clone();

        let mut next = HashMap::new();
        for (id, (path, modified)) in found {
            if let Some(existing) = current.get(&id).filter(|s| s.modified == modified) {
                next.insert(id, existing.clone());
                continue;
            }
            match read_meta(&path, &id) {
                Ok(meta) => {
                    tracing::info!("Tile source '{id}': {} z{}–{} ({})", meta.format, meta.minzoom, meta.maxzoom, path.display());
                    next.insert(id.clone(), Arc::new(TileSource { id, path, modified, meta }));
                }
                Err(e) => tracing::warn!("Skipping tileset {}: {e:#}", path.display()),
            }
        }
        *self.sources.write().unwrap_or_else(|e| e.into_inner()) = next;
        true
    }

    pub fn get(&self, id: &str) -> Option<Arc<TileSource>> {
        self.sources.read().unwrap_or_else(|e| e.into_inner()).get(id).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.sources.read().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
        names.sort();
        names
    }

    pub fn summaries(&self) -> Vec<serde_json::Value> {
        let sources = self.sources.read().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<_> = sources
            .values()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "minzoom": s.meta.minzoom,
                    "maxzoom": s.meta.maxzoom,
                    "format": s.meta.format,
                    "attribution": s.meta.attribution,
                    "layers": s.meta.vector_layers.as_ref()
                        .and_then(|l| l.as_array())
                        .map(|l| l.iter().filter_map(|v| v.get("id").cloned()).collect::<Vec<_>>())
                        .unwrap_or_default(),
                })
            })
            .collect();
        out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        out
    }
}

pub async fn tilejson(State(state): State<Arc<AppState>>, Path(source): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(src) = state.tiles.get(&source) else {
        let layer = live_tiles::find(&source).ok_or(StatusCode::NOT_FOUND)?;
        let backbone = state.tiles.get(&format!("{}-backbone", layer.id));
        return Ok(Json(serde_json::json!({
            "tilejson": "3.0.0",
            "name": layer.id,
            "scheme": "xyz",
            "format": "pbf",
            "tiles": [format!("/tiles/{}/{{z}}/{{x}}/{{y}}", layer.id)],
            "minzoom": backbone.map_or(live_tiles::FETCH_ZOOM, |b| b.meta.minzoom as u32),
            "maxzoom": live_tiles::MAX_ZOOM,
            "bounds": [-180.0, -85.051129, 180.0, 85.051129],
            "vector_layers": [{ "id": layer.layer, "fields": {} }],
            "attribution": "© OpenStreetMap contributors",
        })));
    };
    let m = &src.meta;
    let ext = match m.format.as_str() {
        "pbf" | "mvt" => "",
        other => other,
    };
    let suffix = if ext.is_empty() { String::new() } else { format!(".{ext}") };
    let mut json = serde_json::json!({
        "tilejson": "3.0.0",
        "name": m.name,
        "scheme": "xyz",
        "format": m.format,
        "tiles": [format!("/tiles/{}/{{z}}/{{x}}/{{y}}{suffix}", src.id)],
        "minzoom": m.minzoom,
        "maxzoom": m.maxzoom,
        "bounds": m.bounds.unwrap_or([-180.0, -85.051129, 180.0, 85.051129]),
    });
    if let Some(layers) = &m.vector_layers {
        json["vector_layers"] = layers.clone();
    }
    if let Some(attribution) = &m.attribution {
        json["attribution"] = attribution.clone().into();
    }
    Ok(Json(json))
}

/// `y` may carry an extension (`12.pbf`, `12.png`).
fn parse_row(y: &str) -> Option<u32> {
    y.split('.').next()?.parse().ok()
}

pub fn tms_row(z: u32, x: u32, y: u32) -> Option<u32> {
    if z > 30 {
        return None;
    }
    let size = 1u32 << z;
    if x >= size || y >= size {
        return None;
    }
    Some(size - 1 - y)
}

fn content_type(format: &str) -> &'static str {
    match format {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "application/vnd.mapbox-vector-tile",
    }
}

pub async fn get_tile(State(state): State<Arc<AppState>>, Path((source, z, x, y)): Path<(String, u32, u32, String)>) -> Response {
    let (Some(y), true) = (parse_row(&y), z <= 30) else { return StatusCode::BAD_REQUEST.into_response() };
    let Some(tms_y) = tms_row(z, x, y) else { return StatusCode::BAD_REQUEST.into_response() };
    match state.tiles.get(&source) {
        Some(src) => mbtiles_tile(src, z, x, y, tms_y).await,
        None => match live_tiles::find(&source) {
            Some(layer) => live_tile(&state, layer, z, x, y, tms_y).await,
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

/// Network layer without a built tileset: the backbone below the live zooms, Overpass above.
async fn live_tile(state: &Arc<AppState>, layer: &'static live_tiles::LiveLayer, z: u32, x: u32, y: u32, tms_y: u32) -> Response {
    if z < live_tiles::FETCH_ZOOM {
        return match state.tiles.get(&format!("{}-backbone", layer.id)) {
            Some(backbone) => mbtiles_tile(backbone, z, x, y, tms_y).await,
            None => StatusCode::NO_CONTENT.into_response(),
        };
    }
    if z > live_tiles::MAX_ZOOM {
        return StatusCode::NO_CONTENT.into_response();
    }
    match live_tiles::tile(state, layer, z, x, y).await {
        Ok(Some(data)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/vnd.mapbox-vector-tile"),
                (header::CONTENT_ENCODING, "gzip"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            data,
        )
            .into_response(),
        Ok(None) => (StatusCode::NO_CONTENT, [(header::CACHE_CONTROL, "public, max-age=86400")]).into_response(),
        Err(e) => {
            tracing::warn!("Live {} tile {z}/{x}/{y} unavailable: {e}", layer.id);
            (StatusCode::SERVICE_UNAVAILABLE, [(header::CACHE_CONTROL, "no-store")]).into_response()
        }
    }
}

async fn mbtiles_tile(src: Arc<TileSource>, z: u32, x: u32, y: u32, tms_y: u32) -> Response {
    let source = src.id.clone();
    if z < src.meta.minzoom as u32 || z > src.meta.maxzoom as u32 {
        return StatusCode::NO_CONTENT.into_response();
    }

    let path = src.path.clone();
    let result = tokio::task::spawn_blocking(move || -> rusqlite::Result<Option<Vec<u8>>> {
        open_read_only(&path)?
            .query_row(
                "SELECT tile_data FROM tiles WHERE zoom_level = ?1 AND tile_column = ?2 AND tile_row = ?3",
                rusqlite::params![z, x, tms_y],
                |row| row.get(0),
            )
            .optional()
    })
    .await;

    match result {
        Ok(Ok(Some(data))) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(&src.meta.format)));
            if data.starts_with(&[0x1f, 0x8b]) {
                headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            }
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));
            (StatusCode::OK, headers, data).into_response()
        }
        // No data for this tile: 204 tells MapLibre the tile is empty (404 would be an error).
        Ok(Ok(None)) => (StatusCode::NO_CONTENT, [(header::CACHE_CONTROL, "public, max-age=3600")]).into_response(),
        Ok(Err(e)) => {
            tracing::warn!("Tile read failed for {source}/{z}/{x}/{y}: {e}");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_coordinates_are_validated_before_flipping() {
        assert_eq!(tms_row(0, 0, 0), Some(0));
        assert_eq!(tms_row(3, 5, 0), Some(7));
        assert_eq!(tms_row(3, 5, 7), Some(0));
        assert_eq!(tms_row(3, 8, 0), None);
        assert_eq!(tms_row(3, 0, 8), None);
        assert_eq!(tms_row(31, 0, 0), None);
        assert_eq!(tms_row(u32::MAX, 0, 0), None);
    }

    #[test]
    fn rows_accept_extensions_and_ids_are_restricted() {
        assert_eq!(parse_row("12"), Some(12));
        assert_eq!(parse_row("12.pbf"), Some(12));
        assert_eq!(parse_row("x.pbf"), None);
        assert!(valid_source_id("power-grid") && valid_source_id("hv_lines"));
        assert!(!valid_source_id("../etc") && !valid_source_id("a b") && !valid_source_id(""));
    }

    #[test]
    fn index_discovers_and_forgets_tilesets() {
        let dir = std::env::temp_dir().join(format!("worldmap-tiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let index = TileIndex::new(dir.clone());
        assert!(index.names().is_empty());

        let path = dir.join("demo.mbtiles");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE metadata (name TEXT, value TEXT);
                 CREATE TABLE tiles (zoom_level INTEGER, tile_column INTEGER, tile_row INTEGER, tile_data BLOB);
                 INSERT INTO metadata VALUES ('format', 'pbf'), ('json', '{\"vector_layers\":[{\"id\":\"lines\"}]}');
                 INSERT INTO tiles VALUES (2, 1, 1, x'00'), (6, 1, 1, x'00');",
            )
            .unwrap();
        }
        // A broken file next to it is skipped rather than failing the scan.
        std::fs::write(dir.join("broken.mbtiles"), b"not sqlite").unwrap();
        assert!(index.rescan());
        assert_eq!(index.names(), vec!["demo".to_string()]);
        let meta = &index.get("demo").unwrap().meta;
        assert_eq!((meta.minzoom, meta.maxzoom), (2, 6));
        assert_eq!(index.summaries()[0]["layers"], serde_json::json!(["lines"]));

        std::fs::remove_file(&path).unwrap();
        index.rescan();
        assert!(index.get("demo").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
