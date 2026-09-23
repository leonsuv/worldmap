//! CSV exports and the situation report.

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write as _;
use std::sync::Arc;

use super::events::{affected_assets, cached_flights, load_events};
use super::watchlist::load_items;
use super::{ApiError, ApiResult};
use crate::state::AppState;

/// id, event_id, title, message, severity, acknowledged, created_at
type AlertRow = (i64, Option<i64>, String, String, String, bool, i64);

#[derive(Deserialize)]
pub struct ExportQuery {
    /// ships, events, alerts, watchlist or affected
    pub r#type: String,
    pub event_id: Option<i64>,
}

/// Quote a CSV field and neutralise spreadsheet formulas.
pub fn field(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{value}") } else { value.to_string() };
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_default()
}

fn iso(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0).map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string()).unwrap_or_default()
}

fn value_str(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => field(s),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn csv(filename: &str, body: String) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        // A BOM makes spreadsheet applications detect UTF-8.
        format!("\u{feff}{body}"),
    )
        .into_response()
}

pub async fn export_csv(State(state): State<Arc<AppState>>, Query(q): Query<ExportQuery>) -> ApiResult<Response> {
    let date = chrono::Utc::now().format("%Y-%m-%d");
    match q.r#type.as_str() {
        "ships" => {
            let mut out =
                String::from("mmsi,name,lat,lon,speed_kn,course,heading,ship_type,imo,callsign,destination,eta,length_m,last_report\n");
            let mut ships = state.ais.all_ships();
            ships.sort_by_key(|s| s.mmsi);
            for s in ships {
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    s.mmsi,
                    field(&s.name),
                    s.lat,
                    s.lon,
                    opt(s.speed),
                    opt(s.course),
                    opt(s.heading),
                    opt(s.ship_type),
                    opt(s.imo),
                    field(s.callsign.as_deref().unwrap_or("")),
                    field(s.destination.as_deref().unwrap_or("")),
                    field(s.eta.as_deref().unwrap_or("")),
                    opt(s.length),
                    iso(s.timestamp)
                );
            }
            Ok(csv(&format!("vessels-{date}.csv"), out))
        }
        "events" => {
            let events = state.cache_db.run(|conn| load_events(conn, false)).await?;
            let mut out = String::from("id,name,type,lat,lon,radius_km,description,started,ended,active\n");
            for e in events {
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{},{},{}",
                    e.id,
                    field(&e.name),
                    e.event_type,
                    e.lat,
                    e.lon,
                    e.radius_km,
                    field(&e.description),
                    iso(e.started_at),
                    e.ended_at.map(iso).unwrap_or_default(),
                    e.active
                );
            }
            Ok(csv(&format!("events-{date}.csv"), out))
        }
        "alerts" => {
            let rows: Vec<AlertRow> = state
                .cache_db
                .run(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT id, event_id, title, message, severity, acknowledged, created_at FROM alerts ORDER BY created_at DESC",
                    )?;
                    let rows = stmt
                        .query_map([], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get::<_, i64>(5)? != 0, r.get(6)?))
                        })?
                        .filter_map(Result::ok)
                        .collect();
                    Ok(rows)
                })
                .await?;
            let mut out = String::from("id,event_id,title,message,severity,acknowledged,created\n");
            for (id, event, title, message, severity, ack, created) in rows {
                let _ = writeln!(out, "{id},{},{},{},{severity},{ack},{}", opt(event), field(&title), field(&message), iso(created));
            }
            Ok(csv(&format!("alerts-{date}.csv"), out))
        }
        "watchlist" => {
            let items = state.cache_db.run(load_items).await?;
            let mut out = String::from("id,type,name,mmsi,lat,lon,radius_km,created\n");
            for i in items {
                let p = &i.params;
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{}",
                    i.id,
                    i.wtype,
                    field(&i.name),
                    value_str(p, "mmsi"),
                    value_str(p, "lat"),
                    value_str(p, "lon"),
                    value_str(p, "radius_km"),
                    iso(i.created_at)
                );
            }
            Ok(csv(&format!("watchlist-{date}.csv"), out))
        }
        "affected" => {
            let id = q.event_id.ok_or_else(|| ApiError::bad_request("affected export needs event_id"))?;
            let events = state.cache_db.run(|conn| load_events(conn, false)).await?;
            let e = events.into_iter().find(|e| e.id == id).ok_or_else(|| ApiError::not_found("Event not found"))?;
            let flights = cached_flights(&state).await;
            let a = affected_assets(&state, e.lat, e.lon, e.radius_km, flights.as_ref());
            let mut out = String::from("category,id,name,lat,lon,distance_km\n");
            let groups: [(&str, &Vec<Value>, &str, &str); 5] = [
                ("vessel", &a.ships, "mmsi", "name"),
                ("aircraft", &a.flights, "icao24", "callsign"),
                ("airport", &a.airports, "ident", "name"),
                ("seaport", &a.seaports, "locode", "name"),
                ("nuclear_plant", &a.reactors, "name", "name"),
            ];
            for (category, items, id_key, name_key) in groups {
                for item in items {
                    let _ = writeln!(
                        out,
                        "{category},{},{},{},{},{}",
                        value_str(item, id_key),
                        value_str(item, name_key),
                        value_str(item, "lat"),
                        value_str(item, "lon"),
                        value_str(item, "distance_km")
                    );
                }
            }
            Ok(csv(&format!("event-{id}-affected-{date}.csv"), out))
        }
        _ => Err(ApiError::bad_request("type must be ships, events, alerts, watchlist or affected")),
    }
}

#[derive(Serialize)]
pub struct EventSummary {
    pub id: i64,
    pub name: String,
    pub event_type: String,
    pub lat: f64,
    pub lon: f64,
    pub radius_km: f64,
    pub description: String,
    pub started_at: i64,
    pub vessels: usize,
    pub aircraft: usize,
    pub airports: usize,
    pub seaports: usize,
    pub nuclear_plants: usize,
}

pub async fn situation_report(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let (events, alerts_open, watch_count): (Vec<_>, i64, i64) = state
        .cache_db
        .run(|conn| {
            let events = load_events(conn, true)?;
            let alerts: i64 = conn.query_row("SELECT COUNT(*) FROM alerts WHERE acknowledged = 0", [], |r| r.get(0))?;
            let watch: i64 = conn.query_row("SELECT COUNT(*) FROM watchlist", [], |r| r.get(0))?;
            Ok((events, alerts, watch))
        })
        .await?;
    let flights = cached_flights(&state).await;
    let summaries: Vec<EventSummary> = events
        .into_iter()
        .map(|e| {
            let a = affected_assets(&state, e.lat, e.lon, e.radius_km, flights.as_ref());
            EventSummary {
                id: e.id,
                name: e.name,
                event_type: e.event_type,
                lat: e.lat,
                lon: e.lon,
                radius_km: e.radius_km,
                description: e.description,
                started_at: e.started_at,
                vessels: a.ships.len(),
                aircraft: a.flights.len(),
                airports: a.airports.len(),
                seaports: a.seaports.len(),
                nuclear_plants: a.reactors.len(),
            }
        })
        .collect();
    let ds = state.datasets();
    Ok(Json(serde_json::json!({
        "generated_at": chrono::Utc::now().timestamp(),
        "vessels_tracked": state.ais.ships.len(),
        "aircraft_tracked": flights.as_ref().and_then(|f| f.get("rows")).and_then(Value::as_array).map(|r| r.len()),
        "airports": ds.airports.len(),
        "seaports": ds.seaports.len(),
        "nuclear_plants": ds.plants.len(),
        "active_events": summaries,
        "unacknowledged_alerts": alerts_open,
        "watchlist_items": watch_count,
    })))
}

#[cfg(test)]
mod tests {
    use super::field;

    #[test]
    fn csv_fields_are_quoted_and_formula_safe() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("=HYPERLINK(1)"), "'=HYPERLINK(1)");
        assert_eq!(field("-5"), "'-5");
    }
}
