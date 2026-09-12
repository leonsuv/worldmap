use axum::{extract::State, http::header, response::IntoResponse};
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

/// Serialize and compress immutable datasets once, rather than per request.
pub struct StaticBody {
    identity: axum::body::Bytes,
    gzip: axum::body::Bytes,
}

impl StaticBody {
    pub fn new(value: &serde_json::Value) -> anyhow::Result<Self> {
        use std::io::Write;
        let identity = serde_json::to_vec(value)?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&identity)?;
        Ok(Self { identity: identity.into(), gzip: encoder.finish()?.into() })
    }

    fn response(&self, headers: &axum::http::HeaderMap) -> axum::response::Response {
        let gzip = accepts_gzip(headers);
        (
            [
                (header::CONTENT_TYPE, "application/geo+json"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
                (header::VARY, "Accept-Encoding"),
                (header::CONTENT_ENCODING, if gzip { "gzip" } else { "identity" }),
            ],
            if gzip { self.gzip.clone() } else { self.identity.clone() },
        ).into_response()
    }
}

fn accepts_gzip(headers: &axum::http::HeaderMap) -> bool {
    let mut wildcard = false;
    for value in headers.get_all(header::ACCEPT_ENCODING) {
        let Ok(value) = value.to_str() else { continue };
        for encoding in value.split(',') {
            let mut parts = encoding.trim().split(';');
            let name = parts.next().unwrap_or("").trim();
            let mut quality = 1.0_f32;
            for param in parts {
                if let Some((key, value)) = param.trim().split_once('=') {
                    if key.trim().eq_ignore_ascii_case("q") {
                        quality = value.trim().parse().unwrap_or(0.0);
                    }
                }
            }
            if name.eq_ignore_ascii_case("gzip") { return quality > 0.0; }
            if name == "*" { wildcard = quality > 0.0; }
        }
    }
    wildcard
}

pub async fn get_airports(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    state.airports_body.response(&headers)
}

pub async fn get_seaports(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    state.seaports_body.response(&headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gzip_negotiation_honors_disabled_and_wildcard_encodings() {
        for (value, expected) in [("gzip, deflate, br", true), ("gzip;q=0, *;q=1", false), ("br", false), ("*;q=0.5", true), ("gzip;q=0.2", true)] {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(header::ACCEPT_ENCODING, value.parse().unwrap());
            assert_eq!(accepts_gzip(&headers), expected, "{value}");
        }
        assert!(!accepts_gzip(&axum::http::HeaderMap::new()));
    }
    #[test]
    fn precompressed_dataset_round_trips() {
        use std::io::Read;
        let body = StaticBody::new(&serde_json::json!({ "features": [1, 2, 3] })).unwrap();
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(body.gzip.as_ref()).read_to_end(&mut decoded).unwrap();
        assert_eq!(decoded.as_slice(), body.identity.as_ref());
    }
}
