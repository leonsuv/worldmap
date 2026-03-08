use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::state::AppState;

const BASE_URL: &str = "https://opensky-network.org/api";
const TOKEN_URL: &str = "https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token";
const TOKEN_REFRESH_MARGIN: i64 = 60;

/// Progressive backoff: doubles each time we get 429, resets on success.
static COOLDOWN_UNTIL: AtomicI64 = AtomicI64::new(0);
static BACKOFF_SECS: AtomicI64 = AtomicI64::new(60);

// ─── OAuth2 token management ───

async fn get_opensky_token(state: &Arc<AppState>) -> Option<String> {
    let (client_id, client_secret) = state.opensky_creds.as_ref()?;
    let now = chrono::Utc::now().timestamp();

    {
        let guard = state.opensky_token.lock().await;
        if let Some((ref token, expires_at)) = *guard {
            if now < expires_at - TOKEN_REFRESH_MARGIN {
                return Some(token.clone());
            }
        }
    }

    let resp = state
        .http_client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
        ])
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!("OpenSky token request failed: {}", resp.status());
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let token = data["access_token"].as_str()?.to_string();
    let expires_in = data["expires_in"].as_i64().unwrap_or(1800);
    let expires_at = now + expires_in;

    tracing::info!("OpenSky token refreshed (expires in {expires_in}s)");

    let mut guard = state.opensky_token.lock().await;
    *guard = Some((token.clone(), expires_at));
    Some(token)
}

// ─── Shared OpenSky fetch with caching, backoff, stale fallback ───

type ApiResult = Result<Json<serde_json::Value>, axum::http::StatusCode>;

/// Fetch from OpenSky with caching + backoff. `cache_key` identifies the
/// request in SQLite, `url` is the full OpenSky URL, `cache_ttl` seconds.
async fn opensky_cached_get(
    state: &Arc<AppState>,
    cache_key: &str,
    url: &str,
    cache_ttl: i64,
) -> ApiResult {
    // 1. Fresh cache
    if let Some(cached) = state.cache_db.cache_get(cache_key)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    {
        let v: serde_json::Value = serde_json::from_str(&cached)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(v));
    }

    // 2. Cooldown active?
    let now = chrono::Utc::now().timestamp();
    if now < COOLDOWN_UNTIL.load(Ordering::Relaxed) {
        return serve_stale_or_empty(state, cache_key);
    }

    // 3. Fetch
    let mut req = state.http_client.get(url);
    if let Some(token) = get_opensky_token(state).await {
        req = req.bearer_auth(token);
    }

    let raw = match req.send().await {
        Ok(r) => {
            if r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let secs = BACKOFF_SECS.load(Ordering::Relaxed);
                COOLDOWN_UNTIL.store(now + secs, Ordering::Relaxed);
                let next = (secs * 2).min(1800);
                BACKOFF_SECS.store(next, Ordering::Relaxed);
                tracing::warn!("OpenSky 429 — backing off for {secs}s (next: {next}s)");
                None
            } else if r.status() == reqwest::StatusCode::NOT_FOUND {
                // 404 = no data for this query (empty result)
                Some(String::new())
            } else if r.status().is_client_error() {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                tracing::warn!("OpenSky API {status} for {url}: {body}");
                None
            } else {
                match r.error_for_status() {
                    Ok(r) => r.text().await.ok(),
                    Err(e) => {
                        tracing::warn!("OpenSky API error: {e}");
                        None
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("OpenSky fetch error: {e}");
            None
        }
    };

    match raw {
        None => serve_stale_or_empty(state, cache_key),
        Some(body) if body.is_empty() => {
            // Empty = 404 / no data → return empty JSON array
            Ok(Json(serde_json::json!([])))
        }
        Some(body) => {
            COOLDOWN_UNTIL.store(0, Ordering::Relaxed);
            BACKOFF_SECS.store(60, Ordering::Relaxed);
            let v: serde_json::Value = serde_json::from_str(&body)
                .unwrap_or(serde_json::json!([]));
            let _ = state.cache_db.cache_set(cache_key, &v.to_string(), cache_ttl);
            Ok(Json(v))
        }
    }
}

fn serve_stale_or_empty(
    state: &Arc<AppState>,
    cache_key: &str,
) -> ApiResult {
    if let Ok(Some(stale)) = state.cache_db.cache_get_stale(cache_key) {
        let v: serde_json::Value = serde_json::from_str(&stale)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(v));
    }
    Ok(Json(serde_json::json!([])))
}

// ─── GET /api/flights — all live state vectors as GeoJSON ───

pub async fn get_flights(
    State(state): State<Arc<AppState>>,
) -> ApiResult {
    const CACHE_KEY: &str = "opensky:states";
    const CACHE_TTL: i64 = 15;

    // Check fresh cache
    if let Some(cached) = state.cache_db.cache_get(CACHE_KEY)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    {
        let v: serde_json::Value = serde_json::from_str(&cached)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(v));
    }

    let now = chrono::Utc::now().timestamp();
    if now < COOLDOWN_UNTIL.load(Ordering::Relaxed) {
        return serve_stale_or_empty(&state, CACHE_KEY);
    }

    let url = format!("{BASE_URL}/states/all");
    let mut req = state.http_client.get(&url);
    if let Some(token) = get_opensky_token(&state).await {
        req = req.bearer_auth(token);
    }

    let raw = match req.send().await {
        Ok(r) => {
            if r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let secs = BACKOFF_SECS.load(Ordering::Relaxed);
                COOLDOWN_UNTIL.store(now + secs, Ordering::Relaxed);
                let next = (secs * 2).min(1800);
                BACKOFF_SECS.store(next, Ordering::Relaxed);
                tracing::warn!("OpenSky 429 — backing off for {secs}s (next: {next}s)");
                None
            } else {
                match r.error_for_status() {
                    Ok(r) => r.text().await.ok(),
                    Err(e) => {
                        tracing::warn!("OpenSky API error: {e}");
                        None
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("OpenSky fetch error: {e}");
            None
        }
    };

    if raw.is_none() {
        return serve_stale_or_empty(&state, CACHE_KEY);
    }

    COOLDOWN_UNTIL.store(0, Ordering::Relaxed);
    BACKOFF_SECS.store(60, Ordering::Relaxed);

    let raw = raw.unwrap();
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(Json(serde_json::json!({
            "type": "FeatureCollection", "features": []
        }))),
    };

    let features: Vec<serde_json::Value> = parsed
        .get("states")
        .and_then(|s| s.as_array())
        .map(|states| {
            states.iter().filter_map(|s| {
                let arr = s.as_array()?;
                let lon = arr.get(5)?.as_f64()?;
                let lat = arr.get(6)?.as_f64()?;
                Some(serde_json::json!({
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [lon, lat] },
                    "properties": {
                        "icao24": arr.get(0).and_then(|v| v.as_str()).unwrap_or(""),
                        "callsign": arr.get(1).and_then(|v| v.as_str()).unwrap_or("").trim(),
                        "origin_country": arr.get(2).and_then(|v| v.as_str()).unwrap_or(""),
                        "baro_altitude": arr.get(7).and_then(|v| v.as_f64()),
                        "velocity": arr.get(9).and_then(|v| v.as_f64()),
                        "true_track": arr.get(10).and_then(|v| v.as_f64()),
                        "vertical_rate": arr.get(11).and_then(|v| v.as_f64()),
                        "on_ground": arr.get(8).and_then(|v| v.as_bool()).unwrap_or(false),
                        "category": arr.get(17).and_then(|v| v.as_i64()).unwrap_or(0),
                    }
                }))
            }).collect()
        })
        .unwrap_or_default();

    let collection = serde_json::json!({
        "type": "FeatureCollection",
        "features": features,
    });

    let _ = state.cache_db.cache_set(CACHE_KEY, &collection.to_string(), CACHE_TTL);
    Ok(Json(collection))
}

// ─── GET /api/flights/track?icao24=...&time=0 — aircraft trajectory ───

#[derive(Deserialize)]
pub struct TrackQuery {
    icao24: String,
    #[serde(default)]
    time: Option<i64>,
}

pub async fn get_track(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TrackQuery>,
) -> ApiResult {
    let icao = q.icao24.to_lowercase();
    let t = q.time.unwrap_or(0);
    let cache_key = format!("opensky:track:{icao}:{t}");
    let url = format!("{BASE_URL}/tracks/all?icao24={icao}&time={t}");
    opensky_cached_get(&state, &cache_key, &url, 60).await
}

// ─── GET /api/flights/arrivals?airport=ICAO&begin=...&end=... ───

#[derive(Deserialize)]
pub struct AirportTimeQuery {
    airport: String,
    begin: i64,
    end: i64,
}

pub async fn get_arrivals(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AirportTimeQuery>,
) -> ApiResult {
    let cache_key = format!("opensky:arr:{}:{}:{}", q.airport, q.begin, q.end);
    let url = format!(
        "{BASE_URL}/flights/arrival?airport={}&begin={}&end={}",
        q.airport, q.begin, q.end
    );
    opensky_cached_get(&state, &cache_key, &url, 300).await
}

// ─── GET /api/flights/departures?airport=ICAO&begin=...&end=... ───

pub async fn get_departures(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AirportTimeQuery>,
) -> ApiResult {
    let cache_key = format!("opensky:dep:{}:{}:{}", q.airport, q.begin, q.end);
    let url = format!(
        "{BASE_URL}/flights/departure?airport={}&begin={}&end={}",
        q.airport, q.begin, q.end
    );
    opensky_cached_get(&state, &cache_key, &url, 300).await
}

// ─── GET /api/flights/aircraft?icao24=...&begin=...&end=... ───

#[derive(Deserialize)]
pub struct AircraftQuery {
    icao24: String,
    begin: i64,
    end: i64,
}

pub async fn get_flights_by_aircraft(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AircraftQuery>,
) -> ApiResult {
    let icao = q.icao24.to_lowercase();
    let cache_key = format!("opensky:aircraft:{icao}:{}:{}", q.begin, q.end);
    let url = format!(
        "{BASE_URL}/flights/aircraft?icao24={icao}&begin={}&end={}",
        q.begin, q.end
    );
    opensky_cached_get(&state, &cache_key, &url, 300).await
}

// ─── GET /api/flights/interval?begin=...&end=... ───

#[derive(Deserialize)]
pub struct IntervalQuery {
    begin: i64,
    end: i64,
}

pub async fn get_flights_interval(
    State(state): State<Arc<AppState>>,
    Query(q): Query<IntervalQuery>,
) -> ApiResult {
    let cache_key = format!("opensky:interval:{}:{}", q.begin, q.end);
    let url = format!(
        "{BASE_URL}/flights/all?begin={}&end={}",
        q.begin, q.end
    );
    opensky_cached_get(&state, &cache_key, &url, 300).await
}
