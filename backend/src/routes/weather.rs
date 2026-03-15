use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::cache_proxy::cached_fetch;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct WeatherQuery {
    lat: f64,
    lon: f64,
}

pub async fn get_weather(
    State(state): State<Arc<AppState>>,
    Query(q): Query<WeatherQuery>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let cache_key = format!("weather:{:.2}:{:.2}", q.lat, q.lon);
    let url = format!(
        concat!(
            "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}",
            "&timezone=auto&windspeed_unit=ms",
            "&current=temperature_2m,relative_humidity_2m,apparent_temperature,precipitation,rain,showers,snowfall,weather_code,cloud_cover,pressure_msl,surface_pressure,wind_speed_10m,wind_direction_10m,wind_gusts_10m,visibility,is_day,wave_height,wave_direction,wave_period",
            "&hourly=temperature_2m,apparent_temperature,relative_humidity_2m,dew_point_2m,precipitation_probability,precipitation,rain,showers,snowfall,weather_code,pressure_msl,surface_pressure,cloud_cover,cloud_cover_low,cloud_cover_mid,cloud_cover_high,visibility,wind_speed_10m,wind_direction_10m,wind_gusts_10m,wave_height,wave_direction,wave_period,swell_wave_height,swell_wave_direction,swell_wave_period,wind_wave_height,wind_wave_direction,wind_wave_period",
            "&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum,rain_sum,showers_sum,snowfall_sum,precipitation_probability_max,wind_speed_10m_max,wind_gusts_10m_max,wind_direction_10m_dominant,sunrise,sunset,uv_index_max",
            "&forecast_hours=24&forecast_days=3"
        ),
        lat = q.lat,
        lon = q.lon,
    );

    let raw = match cached_fetch(&state, &cache_key, &url, 900).await {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("Weather fetch error: {e}");

            // Upstream may throttle (429). Serve stale cache instead of hard-failing.
            let key = cache_key.clone();
            let stale = state.cache_db.run(move |conn| {
                let mut stmt = conn.prepare_cached("SELECT body FROM api_cache WHERE key = ?1")?;
                let result = stmt.query_row(rusqlite::params![key], |row| {
                    let blob: Vec<u8> = row.get(0)?;
                    Ok(String::from_utf8_lossy(&blob).into_owned())
                });
                match result {
                    Ok(body) => Ok(Some(body)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(err) => Err(err.into()),
                }
            }).await.unwrap_or(None);

            match stale {
                Some(body) => {
                    tracing::warn!("Serving stale weather cache for {cache_key}");
                    body
                }
                None => return Err(axum::http::StatusCode::BAD_GATEWAY),
            }
        }
    };

    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    Ok(Json(parsed))
}
