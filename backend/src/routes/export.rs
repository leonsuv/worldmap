use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct ExportQuery {
    /// "ships", "events", "alerts", "watchlist", "affected"
    pub r#type: String,
    /// For affected: event_id
    pub event_id: Option<i64>,
}

/// Export data as CSV with appropriate Content-Disposition header.
pub async fn export_csv(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ExportQuery>,
) -> impl IntoResponse {
    let (filename, csv) = match q.r#type.as_str() {
        "ships" => {
            let mut out = String::from("mmsi,ship_name,lat,lon,speed,course,heading,ship_type,imo,callsign,destination\n");
            for entry in state.ship_store.iter() {
                let s = entry.value();
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{}\n",
                    s.mmsi,
                    csv_escape(&s.ship_name),
                    s.lat, s.lon,
                    opt_f64(s.speed), opt_f64(s.course), opt_f64(s.heading),
                    s.ship_type.map(|v| v.to_string()).unwrap_or_default(),
                    s.imo.map(|v| v.to_string()).unwrap_or_default(),
                    s.callsign.as_deref().unwrap_or(""),
                    csv_escape(s.destination.as_deref().unwrap_or("")),
                ));
            }
            ("ships.csv", out)
        }
        "events" => {
            let conn = state.cache_db.conn();
            let mut stmt = conn.prepare(
                "SELECT id, name, event_type, lat, lon, radius_km, description, started_at, ended_at, active FROM events ORDER BY started_at DESC"
            ).unwrap();
            let mut out = String::from("id,name,event_type,lat,lon,radius_km,description,started_at,ended_at,active\n");
            let mut rows = stmt.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                let id: i64 = row.get(0).unwrap();
                let name: String = row.get(1).unwrap();
                let etype: String = row.get(2).unwrap();
                let lat: f64 = row.get(3).unwrap();
                let lon: f64 = row.get(4).unwrap();
                let radius: f64 = row.get(5).unwrap();
                let desc: String = row.get(6).unwrap_or_default();
                let started: i64 = row.get(7).unwrap();
                let ended: Option<i64> = row.get(8).unwrap();
                let active: bool = row.get::<_, i64>(9).unwrap() != 0;
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{}\n",
                    id, csv_escape(&name), etype, lat, lon, radius,
                    csv_escape(&desc), started,
                    ended.map(|v| v.to_string()).unwrap_or_default(),
                    active,
                ));
            }
            ("events.csv", out)
        }
        "alerts" => {
            let conn = state.cache_db.conn();
            let mut stmt = conn.prepare(
                "SELECT id, event_id, title, message, severity, acknowledged, created_at FROM alerts ORDER BY created_at DESC"
            ).unwrap();
            let mut out = String::from("id,event_id,title,message,severity,acknowledged,created_at\n");
            let mut rows = stmt.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                let id: i64 = row.get(0).unwrap();
                let event_id: Option<i64> = row.get(1).unwrap();
                let title: String = row.get(2).unwrap();
                let message: String = row.get(3).unwrap();
                let severity: String = row.get(4).unwrap();
                let ack: bool = row.get::<_, i64>(5).unwrap() != 0;
                let created: i64 = row.get(6).unwrap();
                out.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    id,
                    event_id.map(|v| v.to_string()).unwrap_or_default(),
                    csv_escape(&title), csv_escape(&message),
                    severity, ack, created,
                ));
            }
            ("alerts.csv", out)
        }
        "watchlist" => {
            let conn = state.cache_db.conn();
            let mut stmt = conn.prepare(
                "SELECT id, wtype, name, params, created_at FROM watchlist ORDER BY created_at DESC"
            ).unwrap();
            let mut out = String::from("id,type,name,params,created_at\n");
            let mut rows = stmt.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                let id: i64 = row.get(0).unwrap();
                let wtype: String = row.get(1).unwrap();
                let name: String = row.get(2).unwrap();
                let params: String = row.get(3).unwrap_or_default();
                let created: i64 = row.get(4).unwrap();
                out.push_str(&format!(
                    "{},{},{},{},{}\n",
                    id, wtype, csv_escape(&name), csv_escape(&params), created,
                ));
            }
            ("watchlist.csv", out)
        }
        _ => return (StatusCode::BAD_REQUEST, "Unknown export type").into_response(),
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{filename}\"")),
        ],
        csv,
    ).into_response()
}

/// Situation report as structured JSON (for PDF rendering on frontend).
#[derive(Serialize)]
pub struct SituationReport {
    pub generated_at: i64,
    pub total_ships: usize,
    pub total_events: i64,
    pub active_events: Vec<EventSummary>,
    pub unacknowledged_alerts: i64,
    pub watchlist_count: i64,
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
    pub affected_count: usize,
}

pub async fn situation_report(
    State(state): State<Arc<AppState>>,
) -> Json<SituationReport> {
    let conn = state.cache_db.conn();
    let now = chrono::Utc::now().timestamp();

    let total_ships = state.ship_store.len();

    let total_events: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap_or(0);
    let unacknowledged_alerts: i64 = conn
        .query_row("SELECT COUNT(*) FROM alerts WHERE acknowledged = 0", [], |r| r.get(0))
        .unwrap_or(0);
    let watchlist_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM watchlist", [], |r| r.get(0))
        .unwrap_or(0);

    // Active events with rough affected count
    let mut stmt = conn.prepare(
        "SELECT id, name, event_type, lat, lon, radius_km, description, started_at FROM events WHERE active = 1"
    ).unwrap();
    let active_events: Vec<EventSummary> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .map(|(id, name, event_type, lat, lon, radius_km, description, started_at)| {
            // Count ships in radius
            let affected_count = state.ship_store.iter().filter(|entry| {
                let s = entry.value();
                haversine_km(lat, lon, s.lat, s.lon) <= radius_km
            }).count();
            EventSummary { id, name, event_type, lat, lon, radius_km, description, started_at, affected_count }
        })
        .collect();

    Json(SituationReport {
        generated_at: now,
        total_ships,
        total_events,
        active_events,
        unacknowledged_alerts,
        watchlist_count,
    })
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn opt_f64(v: Option<f64>) -> String {
    v.map(|f| f.to_string()).unwrap_or_default()
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    r * 2.0 * a.sqrt().asin()
}
