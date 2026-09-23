//! Live vessels: WebSocket stream, snapshot, details and aids to navigation.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

use super::{ApiError, ApiResult};
use crate::state::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| stream(socket, state))
}

/// Send a full snapshot, then forward 1 s batches. A client that falls behind
/// receives a fresh snapshot instead of an inconsistent picture.
async fn stream(mut socket: WebSocket, state: Arc<AppState>) {
    // Subscribe before taking the snapshot so no update is lost in between.
    let mut updates = state.ais.updates.subscribe();
    let snapshot = state.ais.snapshot_json();
    if socket.send(Message::Text(snapshot.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            update = updates.recv() => {
                let text: String = match update {
                    Ok(batch) => batch.to_string(),
                    Err(RecvError::Lagged(_)) => state.ais.snapshot_json(),
                    Err(RecvError::Closed) => break,
                };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                _ => {}
            },
        }
    }
}

/// GET /api/ships/snapshot — every live vessel in the compact row format.
pub async fn snapshot(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")], state.ais.snapshot_json())
}

/// GET /api/ships/{mmsi} — full record including voyage data.
pub async fn details(State(state): State<Arc<AppState>>, Path(mmsi): Path<u64>) -> ApiResult<Json<Value>> {
    let ship =
        state.ais.ships.get(&mmsi).map(|s| s.value().clone()).ok_or_else(|| ApiError::not_found("Vessel is not in the live picture"))?;
    let mut value = serde_json::to_value(&ship).map_err(anyhow::Error::from)?;
    value["age_secs"] = (chrono::Utc::now().timestamp() - ship.timestamp).max(0).into();
    Ok(Json(value))
}

/// GET /api/ships/aton — aids to navigation as GeoJSON.
pub async fn aton(State(state): State<Arc<AppState>>) -> Json<Value> {
    let features: Vec<Value> = state
        .ais
        .aton
        .iter()
        .map(|a| {
            serde_json::json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [a.lon, a.lat] },
                "properties": {
                    "mmsi": a.mmsi,
                    "name": a.name,
                    "aton_type": a.aton_type,
                    "virtual": a.virtual_aton,
                    "off_position": a.off_position,
                    "timestamp": a.timestamp,
                },
            })
        })
        .collect();
    Json(serde_json::json!({ "type": "FeatureCollection", "features": features }))
}
