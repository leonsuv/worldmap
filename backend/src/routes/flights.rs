//! OpenSky Network: live aircraft states, tracks and flight history.
//!
//! OpenSky meters `/states/all` in credits (4 per global request; 400/day
//! anonymous, 4000/day with an API client), so the cache lifetime follows the
//! account type and the client extrapolates positions between updates.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use super::{ApiError, ApiResult};
use crate::state::AppState;
use crate::upstream::{self, Fetched, UpstreamError};

const API: &str = "https://opensky-network.org/api";
const TOKEN_URL: &str = "https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token";

pub const FIELDS: [&str; 13] = [
    "icao24",
    "callsign",
    "country",
    "lon",
    "lat",
    "altitude",
    "velocity",
    "track",
    "vertical_rate",
    "on_ground",
    "category",
    "time_position",
    "squawk",
];

pub fn states_ttl(authenticated: bool) -> i64 {
    if authenticated {
        30
    } else {
        120
    }
}

async fn token(state: &AppState) -> Option<String> {
    let (id, secret) = state.config.opensky_credentials.as_ref()?;
    let now = chrono::Utc::now().timestamp();
    let mut cached = state.opensky_token.lock().await;
    if let Some((token, expires)) = cached.as_ref() {
        if now < expires - 60 {
            return Some(token.clone());
        }
    }
    let response = state
        .http
        .post(TOKEN_URL)
        .timeout(std::time::Duration::from_secs(15))
        .form(&[("grant_type", "client_credentials"), ("client_id", id.as_str()), ("client_secret", secret.as_str())])
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        tracing::warn!("OpenSky token request failed: HTTP {}. Check OPENSKY_CLIENT_ID/SECRET.", response.status());
        return None;
    }
    let data: Value = response.json().await.ok()?;
    let token = data["access_token"].as_str()?.to_string();
    let expires = now + data["expires_in"].as_i64().unwrap_or(1800);
    *cached = Some((token.clone(), expires));
    Some(token)
}

/// GET an OpenSky endpoint through the shared cache. 404 means "no data".
async fn opensky(state: &AppState, key: &str, path: &str, query: &[(&str, String)], ttl: i64) -> Result<Fetched, UpstreamError> {
    upstream::cached(&state.cache_db, &state.providers.opensky, key, ttl, || async {
        let mut request = state.http.get(format!("{API}{path}")).query(query);
        if let Some(token) = token(state).await {
            request = request.bearer_auth(token);
        }
        match upstream::send(request).await {
            Err(UpstreamError::Status(404)) => Ok(b"null".to_vec()),
            other => other,
        }
    })
    .await
}

fn round(v: Option<f64>, places: i32) -> Value {
    let f = 10f64.powi(places);
    v.filter(|x| x.is_finite()).map(|x| Value::from((x * f).round() / f)).unwrap_or(Value::Null)
}

/// Convert an OpenSky `/states/all` response into compact rows.
pub fn states_to_rows(raw: &Value) -> Value {
    let rows: Vec<Value> = raw
        .get("states")
        .and_then(Value::as_array)
        .map(|states| {
            states
                .iter()
                .filter_map(|s| {
                    let a = s.as_array()?;
                    let lon = a.get(5)?.as_f64()?;
                    let lat = a.get(6)?.as_f64()?;
                    if !crate::geo::valid_lat_lon(lat, lon) {
                        return None;
                    }
                    let num = |i: usize| a.get(i).and_then(Value::as_f64);
                    let altitude = num(7).or_else(|| num(13));
                    Some(Value::Array(vec![
                        a.first().cloned().unwrap_or(Value::Null),
                        a.get(1).and_then(Value::as_str).map(|c| Value::from(c.trim())).unwrap_or(Value::Null),
                        a.get(2).cloned().unwrap_or(Value::Null),
                        round(Some(lon), 4),
                        round(Some(lat), 4),
                        round(altitude, 0),
                        round(num(9), 1),
                        round(num(10), 1),
                        round(num(11), 1),
                        Value::from(a.get(8).and_then(Value::as_bool).unwrap_or(false)),
                        Value::from(a.get(17).and_then(Value::as_i64).unwrap_or(0)),
                        a.get(3).cloned().unwrap_or(Value::Null),
                        a.get(14).cloned().unwrap_or(Value::Null),
                    ]))
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!({ "time": raw.get("time").cloned().unwrap_or(Value::Null), "fields": FIELDS, "rows": rows })
}

/// GET /api/flights — all aircraft currently tracked.
pub async fn get_flights(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let authenticated = state.config.opensky_credentials.is_some();
    let ttl = states_ttl(authenticated);
    let fetched = upstream::cached(&state.cache_db, &state.providers.opensky, "opensky:states:v2", ttl, || async {
        let mut request = state.http.get(format!("{API}/states/all")).query(&[("extended", "1")]);
        if let Some(token) = token(&state).await {
            request = request.bearer_auth(token);
        }
        let body = upstream::send(request).await?;
        let raw: Value = serde_json::from_slice(&body).map_err(|e| UpstreamError::Invalid(e.to_string()))?;
        serde_json::to_vec(&states_to_rows(&raw)).map_err(|e| UpstreamError::Invalid(e.to_string()))
    })
    .await?;
    let mut data: Value = serde_json::from_slice(&fetched.body).map_err(|_| ApiError::bad_gateway("Cached flight data is unreadable"))?;
    let now = chrono::Utc::now().timestamp();
    data["fetched_at"] = fetched.fetched_at.into();
    data["stale"] = fetched.stale.into();
    data["authenticated"] = authenticated.into();
    data["refresh_in"] = (fetched.fetched_at + ttl - now).max(5).into();
    Ok(Json(data))
}

fn icao24(value: &str) -> ApiResult<String> {
    let v = value.trim().to_ascii_lowercase();
    if v.len() == 6 && v.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(v)
    } else {
        Err(ApiError::bad_request("icao24 must be a 6-digit hexadecimal transponder address"))
    }
}

fn airport_icao(value: &str) -> ApiResult<String> {
    let v = value.trim().to_ascii_uppercase();
    if v.len() == 4 && v.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(v)
    } else {
        Err(ApiError::bad_request("airport must be a 4-character ICAO code"))
    }
}

/// Validate a history window: `end` after `begin`, at most `max_secs` long.
fn window(begin: Option<i64>, end: Option<i64>, default_secs: i64, max_secs: i64) -> ApiResult<(i64, i64)> {
    let now = chrono::Utc::now().timestamp();
    let end = end.unwrap_or(now).min(now);
    let begin = begin.unwrap_or(end - default_secs);
    if begin >= end || end - begin > max_secs || begin < 0 {
        return Err(ApiError::bad_request(format!("time window must be positive and at most {} hours", max_secs / 3600)));
    }
    Ok((begin, end))
}

#[derive(Deserialize)]
pub struct TrackQuery {
    icao24: String,
}

/// GET /api/flights/track — the current flight path of one aircraft.
pub async fn get_track(State(state): State<Arc<AppState>>, Query(q): Query<TrackQuery>) -> ApiResult<Json<Value>> {
    let icao = icao24(&q.icao24)?;
    let key = format!("opensky:track:{icao}");
    let fetched = opensky(&state, &key, "/tracks/all", &[("icao24", icao.clone()), ("time", "0".into())], 60).await?;
    let raw: Value = serde_json::from_slice(&fetched.body).unwrap_or(Value::Null);
    let path: Vec<Value> = raw
        .get("path")
        .and_then(Value::as_array)
        .map(|p| {
            p.iter()
                .filter_map(|wp| {
                    let w = wp.as_array()?;
                    let lat = w.get(1)?.as_f64()?;
                    let lon = w.get(2)?.as_f64()?;
                    crate::geo::valid_lat_lon(lat, lon)
                        .then(|| serde_json::json!([lon, lat, w.get(3).and_then(Value::as_f64).unwrap_or(0.0)]))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "icao24": icao,
        "callsign": raw.get("callsign").and_then(Value::as_str).map(str::trim),
        "start": raw.get("startTime"),
        "end": raw.get("endTime"),
        "path": path,
        "stale": fetched.stale,
    })))
}

#[derive(Deserialize)]
pub struct AircraftQuery {
    icao24: String,
    begin: Option<i64>,
    end: Option<i64>,
}

/// GET /api/flights/aircraft — flights of one aircraft (OpenSky batch data, updated nightly).
pub async fn get_aircraft_flights(State(state): State<Arc<AppState>>, Query(q): Query<AircraftQuery>) -> ApiResult<Json<Value>> {
    let icao = icao24(&q.icao24)?;
    let (begin, end) = window(q.begin, q.end, 2 * 86400, 2 * 86400)?;
    // Align to the hour so repeated clicks share one cache entry.
    let (begin, end) = (begin - begin % 3600, end - end % 3600 + 3600);
    let key = format!("opensky:aircraft:{icao}:{begin}:{end}");
    let query = [("icao24", icao), ("begin", begin.to_string()), ("end", end.to_string())];
    let fetched = opensky(&state, &key, "/flights/aircraft", &query, 1800).await?;
    let flights: Value = serde_json::from_slice(&fetched.body).unwrap_or(Value::Null);
    Ok(Json(if flights.is_array() { flights } else { Value::Array(vec![]) }))
}

#[derive(Deserialize)]
pub struct AirportQuery {
    airport: String,
    /// `arrivals` or `departures`.
    kind: String,
    begin: Option<i64>,
    end: Option<i64>,
}

/// GET /api/flights/airport — recent arrivals or departures (batch data).
pub async fn get_airport_flights(State(state): State<Arc<AppState>>, Query(q): Query<AirportQuery>) -> ApiResult<Json<Value>> {
    let airport = airport_icao(&q.airport)?;
    let path = match q.kind.as_str() {
        "arrivals" => "/flights/arrival",
        "departures" => "/flights/departure",
        _ => return Err(ApiError::bad_request("kind must be arrivals or departures")),
    };
    // Batch data for the previous day is complete; default to it.
    let now = chrono::Utc::now().timestamp();
    let day_start = now - now % 86400;
    let (begin, end) = window(q.begin.or(Some(day_start - 86400)), q.end.or(Some(day_start)), 86400, 2 * 86400)?;
    let key = format!("opensky:{}:{airport}:{begin}:{end}", q.kind);
    let query = [("airport", airport), ("begin", begin.to_string()), ("end", end.to_string())];
    let fetched = opensky(&state, &key, path, &query, 3600).await?;
    let flights: Value = serde_json::from_slice(&fetched.body).unwrap_or(Value::Null);
    Ok(Json(serde_json::json!({ "begin": begin, "end": end, "flights": if flights.is_array() { flights } else { Value::Array(vec![]) } })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_become_compact_rows() {
        let raw = serde_json::json!({
            "time": 1_700_000_000,
            "states": [
                ["3c6444", "DLH9LF  ", "Germany", 1_699_999_990, 1_699_999_995, 13.12345678, 52.5, 10972.8, false, 231.4, 90.04, -0.3, null, 11000.0, "1000", false, 0, 3],
                ["abcdef", null, "X", null, null, null, null, null, true, null, null, null],
                ["bad", "", "X", 0, 0, 500.0, 95.0, 0, false, 0, 0, 0]
            ]
        });
        let out = states_to_rows(&raw);
        let rows = out["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "rows without a valid position are dropped");
        assert_eq!(rows[0][0], "3c6444");
        assert_eq!(rows[0][1], "DLH9LF");
        assert_eq!(rows[0][3], 13.1235);
        assert_eq!(rows[0][5], 10973.0);
        assert_eq!(rows[0][10], 3);
        assert_eq!(out["fields"].as_array().unwrap().len(), rows[0].as_array().unwrap().len());
    }

    #[test]
    fn identifiers_and_windows_are_validated() {
        assert_eq!(icao24(" 3C6444 ").unwrap(), "3c6444");
        assert!(icao24("3c644").is_err() && icao24("zzzzzz").is_err() && icao24("3c6444&x=1").is_err());
        assert_eq!(airport_icao("eddb").unwrap(), "EDDB");
        assert!(airport_icao("ED DB").is_err() && airport_icao("EDDB1").is_err());
        let now = chrono::Utc::now().timestamp();
        assert!(window(Some(now - 100), Some(now), 10, 3600).is_ok());
        assert!(window(Some(now), Some(now - 100), 10, 3600).is_err());
        assert!(window(Some(now - 7200), Some(now), 10, 3600).is_err());
        assert_eq!(states_ttl(true), 30);
        assert_eq!(states_ttl(false), 120);
    }
}
