use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    Json,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

#[derive(Clone, Serialize, Deserialize)]
pub struct ShipPosition {
    pub mmsi: u64,
    pub lat: f64,
    pub lon: f64,
    pub course: Option<f64>,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
    pub ship_name: String,
    pub ship_type: Option<u32>,
    pub timestamp: i64,
    // Extended static data (from ShipStaticData msg 5 / StaticDataReport msg 24)
    pub imo: Option<u64>,
    pub callsign: Option<String>,
    pub destination: Option<String>,
    pub eta: Option<String>,          // formatted "MM-DD HH:MM"
    pub draught: Option<f64>,
    pub length: Option<u32>,          // Dimension.A + Dimension.B
    pub beam: Option<u32>,            // Dimension.C + Dimension.D
    pub nav_status: Option<u8>,       // NavigationalStatus from position reports
    pub rate_of_turn: Option<i32>,
}

pub type ShipStore = Arc<DashMap<u64, ShipPosition>>;

/// Aids-to-Navigation report (buoys, lighthouses, etc.)
#[derive(Clone, Serialize, Deserialize)]
pub struct AtoNReport {
    pub mmsi: u64,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub aton_type: u32,       // AtoN type code
    pub virtual_aton: bool,
    pub off_position: bool,
    pub timestamp: i64,
}

pub type AtoNStore = Arc<DashMap<u64, AtoNReport>>;

/// SAR aircraft positions
#[derive(Clone, Serialize, Deserialize)]
pub struct SarAircraft {
    pub mmsi: u64,
    pub lat: f64,
    pub lon: f64,
    pub altitude: Option<f64>,
    pub speed: Option<f64>,
    pub course: Option<f64>,
    pub timestamp: i64,
}

pub type SarStore = Arc<DashMap<u64, SarAircraft>>;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.ship_broadcast.subscribe();

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(text) => {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Ship WS client lagged by {n} messages");
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

#[derive(Serialize)]
pub struct GeoJsonFeatureCollection {
    r#type: &'static str,
    features: Vec<serde_json::Value>,
}

pub async fn snapshot(
    State(state): State<Arc<AppState>>,
) -> Json<GeoJsonFeatureCollection> {
    let ship_store = &state.ship_store;
    let features: Vec<serde_json::Value> = ship_store
        .iter()
        .map(|entry| {
            let ship = entry.value();
            let heading = ship.heading.filter(|&h| h < 360.0);
            let course = ship.course.filter(|&c| c < 360.0);
            serde_json::json!({
                "type": "Feature",
                "geometry": {
                    "type": "Point",
                    "coordinates": [ship.lon, ship.lat]
                },
                "properties": {
                    "mmsi": ship.mmsi,
                    "ship_name": ship.ship_name,
                    "ship_type": ship.ship_type,
                    "course": course,
                    "speed": ship.speed,
                    "heading": heading,
                    "imo": ship.imo,
                    "callsign": ship.callsign,
                    "destination": ship.destination,
                    "eta": ship.eta,
                    "draught": ship.draught,
                    "length": ship.length,
                    "beam": ship.beam,
                    "nav_status": ship.nav_status,
                }
            })
        })
        .collect();

    Json(GeoJsonFeatureCollection {
        r#type: "FeatureCollection",
        features,
    })
}

pub async fn aton_snapshot(
    State(state): State<Arc<AppState>>,
) -> Json<GeoJsonFeatureCollection> {
    let features: Vec<serde_json::Value> = state.aton_store
        .iter()
        .map(|entry| {
            let a = entry.value();
            serde_json::json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [a.lon, a.lat] },
                "properties": {
                    "mmsi": a.mmsi,
                    "name": a.name,
                    "aton_type": a.aton_type,
                    "virtual": a.virtual_aton,
                    "off_position": a.off_position,
                }
            })
        })
        .collect();

    Json(GeoJsonFeatureCollection {
        r#type: "FeatureCollection",
        features,
    })
}

pub async fn sar_snapshot(
    State(state): State<Arc<AppState>>,
) -> Json<GeoJsonFeatureCollection> {
    let features: Vec<serde_json::Value> = state.sar_store
        .iter()
        .map(|entry| {
            let s = entry.value();
            serde_json::json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [s.lon, s.lat] },
                "properties": {
                    "mmsi": s.mmsi,
                    "altitude": s.altitude,
                    "speed": s.speed,
                    "course": s.course,
                }
            })
        })
        .collect();

    Json(GeoJsonFeatureCollection {
        r#type: "FeatureCollection",
        features,
    })
}
