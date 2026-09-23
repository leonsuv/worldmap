//! Open-Meteo weather.
//!
//! The map requests a *grid*: sample points sit on a fixed lattice (multiples
//! of a zoom-dependent step), so panning reuses cached points, and all missing
//! points are fetched in one multi-location request.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::Arc;

use super::{ApiError, ApiResult};
use crate::geo::{valid_lat_lon, wrap_lon, BBox};
use crate::state::AppState;
use crate::upstream::{self, UpstreamError};

const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
const CURRENT: &str = "temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,weather_code,cloud_cover,pressure_msl,wind_speed_10m,wind_direction_10m,wind_gusts_10m";
const POINT_TTL: i64 = 900;
const MAX_POINTS: usize = 60;
const BATCH: usize = 50;
const STEPS: [f64; 20] = [0.25, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.5, 10.0, 12.0, 15.0, 18.0, 20.0, 24.0, 30.0, 36.0, 45.0];

/// Sample points for a viewport: the finest lattice with at most `max_points`.
pub fn lattice(bbox: &BBox, max_points: usize) -> (f64, Vec<(f64, f64)>) {
    let south = bbox.south.max(-80.0);
    let north = bbox.north.min(80.0);
    let span = bbox.lon_span();
    for step in STEPS {
        let lats: Vec<f64> = axis(south, north, step);
        let lons: Vec<f64> = axis(bbox.west, bbox.west + span, step);
        if lats.len() * lons.len() > max_points {
            continue;
        }
        let mut points = Vec::with_capacity(lats.len() * lons.len());
        let mut seen = std::collections::HashSet::new();
        for &lat in &lats {
            for &lon in &lons {
                let lon = wrap_lon(lon);
                if seen.insert(((lat * 1000.0) as i64, (lon * 1000.0) as i64)) {
                    points.push((lat, lon));
                }
            }
        }
        return (step, points);
    }
    (STEPS[STEPS.len() - 1], vec![bbox.center()])
}

/// Lattice coordinates (cell centres) between `from` and `to`.
fn axis(from: f64, to: f64, step: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut k = (from / step - 0.5).ceil();
    loop {
        let v = (k + 0.5) * step;
        if v > to || out.len() > 400 {
            break;
        }
        out.push((v * 1000.0).round() / 1000.0);
        k += 1.0;
    }
    if out.is_empty() {
        // The view is narrower than one cell: use the cell containing its centre.
        let mid = (from + to) / 2.0;
        out.push(((((mid / step).floor() + 0.5) * step) * 1000.0).round() / 1000.0);
    }
    out
}

fn point_key(lat: f64, lon: f64) -> String {
    format!("weather:v3:{lat:.3}:{lon:.3}")
}

fn number(v: &Value, key: &str) -> Value {
    v.get(key).and_then(Value::as_f64).map(Value::from).unwrap_or(Value::Null)
}

/// The compact per-point record stored in the cache and returned to clients.
pub fn point_record(lat: f64, lon: f64, current: &Value) -> Value {
    let mut m = Map::new();
    m.insert("lat".into(), lat.into());
    m.insert("lon".into(), lon.into());
    for (out, key) in [
        ("temperature", "temperature_2m"),
        ("apparent_temperature", "apparent_temperature"),
        ("humidity", "relative_humidity_2m"),
        ("precipitation", "precipitation"),
        ("weather_code", "weather_code"),
        ("cloud_cover", "cloud_cover"),
        ("pressure", "pressure_msl"),
        ("wind_speed", "wind_speed_10m"),
        ("wind_direction", "wind_direction_10m"),
        ("wind_gusts", "wind_gusts_10m"),
        ("is_day", "is_day"),
    ] {
        m.insert(out.into(), number(current, key));
    }
    m.insert("time".into(), current.get("time").cloned().unwrap_or(Value::Null));
    Value::Object(m)
}

async fn fetch_batch(state: &AppState, points: &[(f64, f64)]) -> Result<Vec<Value>, UpstreamError> {
    let join = |f: fn(&(f64, f64)) -> f64| points.iter().map(|p| format!("{:.3}", f(p))).collect::<Vec<_>>().join(",");
    let request = state.http.get(FORECAST_URL).query(&[
        ("latitude", join(|p| p.0)),
        ("longitude", join(|p| p.1)),
        ("current", CURRENT.to_string()),
        ("wind_speed_unit", "ms".to_string()),
        ("timezone", "GMT".to_string()),
    ]);
    let body = upstream::send(request).await?;
    let data: Value = serde_json::from_slice(&body).map_err(|e| UpstreamError::Invalid(e.to_string()))?;
    let items = match data {
        Value::Array(items) => items,
        single @ Value::Object(_) => vec![single],
        _ => return Err(UpstreamError::Invalid("unexpected weather payload".into())),
    };
    if items.len() != points.len() {
        return Err(UpstreamError::Invalid(format!("expected {} locations, got {}", points.len(), items.len())));
    }
    Ok(points.iter().zip(items).map(|(&(lat, lon), item)| point_record(lat, lon, item.get("current").unwrap_or(&Value::Null))).collect())
}

#[derive(Deserialize)]
pub struct GridQuery {
    bbox: String,
}

/// GET /api/weather/grid?bbox=w,s,e,n
pub async fn get_grid(State(state): State<Arc<AppState>>, Query(q): Query<GridQuery>) -> ApiResult<Json<Value>> {
    let bbox = BBox::parse(&q.bbox).ok_or_else(|| ApiError::bad_request("bbox must be west,south,east,north"))?;
    let (step, points) = lattice(&bbox, MAX_POINTS);

    let keys: Vec<String> = points.iter().map(|&(lat, lon)| point_key(lat, lon)).collect();
    let db = state.cache_db.clone();
    let lookup = keys.clone();
    let entries = tokio::task::spawn_blocking(move || lookup.iter().map(|k| db.cache_get(k).ok().flatten()).collect::<Vec<_>>())
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut results: Vec<Option<Value>> = vec![None; points.len()];
    let mut missing = Vec::new();
    let mut stale = false;
    let mut oldest = chrono::Utc::now().timestamp();
    for (i, entry) in entries.iter().enumerate() {
        match entry {
            Some(e) if e.fresh => {
                results[i] = serde_json::from_slice(&e.body).ok();
                oldest = oldest.min(e.fetched_at);
            }
            _ => missing.push(i),
        }
    }

    let now = chrono::Utc::now().timestamp();
    let mut error = None;
    if !missing.is_empty() {
        if let Some(wait) = state.providers.open_meteo.cooling_down(now) {
            error = Some(UpstreamError::RateLimited { retry_after: Some(wait) });
        } else {
            for chunk in missing.chunks(BATCH) {
                let batch: Vec<(f64, f64)> = chunk.iter().map(|&i| points[i]).collect();
                match fetch_batch(&state, &batch).await {
                    Ok(records) => {
                        state.providers.open_meteo.on_success();
                        let to_store: Vec<(String, Vec<u8>)> = chunk
                            .iter()
                            .zip(&records)
                            .map(|(&i, r)| (keys[i].clone(), serde_json::to_vec(r).unwrap_or_default()))
                            .collect();
                        let db = state.cache_db.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            for (k, v) in to_store {
                                let _ = db.cache_set(&k, &v, POINT_TTL);
                            }
                        })
                        .await;
                        for (&i, r) in chunk.iter().zip(records) {
                            results[i] = Some(r);
                        }
                    }
                    Err(e) => {
                        if let UpstreamError::RateLimited { retry_after } = &e {
                            state.providers.open_meteo.on_rate_limit(now, *retry_after);
                        } else {
                            tracing::warn!("Open-Meteo request failed: {e}");
                        }
                        error = Some(e);
                        break;
                    }
                }
            }
        }
        // Fill anything still missing from expired cache entries.
        for &i in &missing {
            if results[i].is_none() {
                if let Some(e) = &entries[i] {
                    results[i] = serde_json::from_slice(&e.body).ok();
                    stale = true;
                    oldest = oldest.min(e.fetched_at);
                }
            }
        }
    }

    let points: Vec<Value> = results.into_iter().flatten().collect();
    if points.is_empty() {
        return Err(match error {
            Some(e) => e.into(),
            None => ApiError::bad_gateway("No weather data available"),
        });
    }
    Ok(Json(serde_json::json!({ "step": step, "points": points, "stale": stale || error.is_some(), "fetched_at": oldest })))
}

#[derive(Deserialize)]
pub struct PointQuery {
    lat: f64,
    lon: f64,
}

/// GET /api/weather?lat=&lon= — current conditions and a 24-hour outlook.
pub async fn get_point(State(state): State<Arc<AppState>>, Query(q): Query<PointQuery>) -> ApiResult<Json<Value>> {
    if !valid_lat_lon(q.lat, q.lon) {
        return Err(ApiError::bad_request("lat must be within ±90 and lon within ±180"));
    }
    let (lat, lon) = ((q.lat * 100.0).round() / 100.0, (q.lon * 100.0).round() / 100.0);
    let key = format!("weather:point:v3:{lat:.2}:{lon:.2}");
    let fetched = upstream::cached(&state.cache_db, &state.providers.open_meteo, &key, POINT_TTL, || async {
        let request = state.http.get(FORECAST_URL).query(&[
            ("latitude", format!("{lat:.2}")),
            ("longitude", format!("{lon:.2}")),
            ("current", CURRENT.to_string()),
            ("hourly", "temperature_2m,precipitation_probability,wind_speed_10m,weather_code".to_string()),
            ("forecast_hours", "24".to_string()),
            ("wind_speed_unit", "ms".to_string()),
            ("timezone", "auto".to_string()),
        ]);
        upstream::send(request).await
    })
    .await?;
    let raw: Value = serde_json::from_slice(&fetched.body).map_err(|_| ApiError::bad_gateway("Unreadable weather response"))?;
    let mut out = point_record(lat, lon, raw.get("current").unwrap_or(&Value::Null));
    out["hourly"] = raw.get("hourly").cloned().unwrap_or(Value::Null);
    out["timezone"] = raw.get("timezone").cloned().unwrap_or(Value::Null);
    out["stale"] = fetched.stale.into();
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(s: &str) -> BBox {
        BBox::parse(s).unwrap()
    }

    #[test]
    fn lattice_is_bounded_and_covers_the_view() {
        for view in ["-180,-85,180,85", "-400,-85,400,85", "5,47,15,55", "170,-20,210,20", "13.3,52.4,13.5,52.6"] {
            let (step, points) = lattice(&bbox(view), MAX_POINTS);
            assert!(!points.is_empty() && points.len() <= MAX_POINTS, "{view}: {}", points.len());
            assert!(points.iter().all(|&(lat, lon)| lat.abs() <= 80.0 && (-180.0..180.0).contains(&lon)), "{view}");
            assert!(STEPS.contains(&step));
        }
        let (_, wrapped) = lattice(&bbox("170,-20,210,20"), MAX_POINTS);
        assert!(wrapped.iter().any(|p| p.1 > 170.0) && wrapped.iter().any(|p| p.1 < -150.0));
    }

    #[test]
    fn panning_reuses_lattice_points() {
        let (s1, a) = lattice(&bbox("0,40,20,50"), MAX_POINTS);
        let (s2, b) = lattice(&bbox("1,40.5,21,50.5"), MAX_POINTS);
        assert_eq!(s1, s2);
        let shared = a.iter().filter(|p| b.contains(p)).count();
        assert!(shared * 10 >= a.len() * 7, "only {shared} of {} points reused", a.len());
    }

    #[test]
    fn records_keep_only_known_fields() {
        let current =
            serde_json::json!({ "temperature_2m": 12.5, "wind_speed_10m": 4.2, "wind_direction_10m": 270, "time": "2026-09-23T10:00" });
        let r = point_record(52.5, 13.5, &current);
        assert_eq!(r["temperature"], 12.5);
        assert_eq!(r["wind_direction"], 270.0);
        assert!(r["humidity"].is_null());
        assert_eq!(r["time"], "2026-09-23T10:00");
    }
}
