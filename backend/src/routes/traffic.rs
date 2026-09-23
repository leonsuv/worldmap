//! TomTom traffic-flow raster tiles, proxied so the API key stays on the server.

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::sync::{Arc, LazyLock};

use super::tiles::tms_row;
use crate::state::AppState;
use crate::upstream;

const TILE_TTL: i64 = 120;

#[derive(Deserialize)]
pub struct TrafficQuery {
    /// `dark` or `light` basemap.
    style: Option<String>,
}

/// A 1×1 transparent PNG, returned instead of an error so the map stays quiet.
static EMPTY_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    use std::io::Write;
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut png, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&[0, 0, 0, 0, 0]).expect("in-memory write");
    chunk(&mut png, b"IDAT", &z.finish().expect("in-memory write"));
    chunk(&mut png, b"IEND", &[]);
    png
});

fn png(body: Vec<u8>, max_age: u32) -> Response {
    (StatusCode::OK, [(header::CONTENT_TYPE, "image/png".to_string()), (header::CACHE_CONTROL, format!("public, max-age={max_age}"))], body)
        .into_response()
}

/// GET /api/traffic/{z}/{x}/{y}
pub async fn get_tile(
    State(state): State<Arc<AppState>>,
    Path((z, x, y)): Path<(u32, u32, String)>,
    Query(q): Query<TrafficQuery>,
) -> Response {
    let Some(key) = state.config.tomtom_key.clone() else {
        return super::ApiError::unavailable("Road traffic needs TOMTOM_API_KEY in backend/.env").into_response();
    };
    let y: Option<u32> = y.split('.').next().and_then(|v| v.parse().ok());
    let (Some(y), true) = (y, z <= 22) else { return StatusCode::BAD_REQUEST.into_response() };
    if tms_row(z, x, y).is_none() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let style = if q.style.as_deref() == Some("dark") { "relative0-dark" } else { "relative0" };
    let cache_key = format!("traffic:{style}:{z}:{x}:{y}");
    let url = format!("https://api.tomtom.com/traffic/map/4/tile/flow/{style}/{z}/{x}/{y}.png");
    let result = upstream::cached(&state.cache_db, &state.providers.tomtom, &cache_key, TILE_TTL, || async {
        upstream::send(state.http.get(&url).query(&[("key", key.as_str()), ("tileSize", "256")])).await
    })
    .await;
    match result {
        Ok(fetched) if fetched.body.starts_with(b"\x89PNG") => png(fetched.body, 60),
        Ok(_) => png(EMPTY_PNG.clone(), 30),
        Err(e) => {
            tracing::debug!("Traffic tile {z}/{x}/{y} unavailable: {e}");
            png(EMPTY_PNG.clone(), 30)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_png_is_well_formed() {
        let png = &*super::EMPTY_PNG;
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]), "IEND CRC");
        assert_eq!(&png[12..16], b"IHDR");
    }
}
