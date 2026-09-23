//! Imported datasets as pre-compressed GeoJSON.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;

use crate::datasets::StaticBody;
use crate::state::AppState;

fn respond(body: Option<&StaticBody>, headers: &HeaderMap) -> Response {
    match body {
        Some(body) => body.response(headers, 3600),
        // Not imported yet: an empty collection keeps clients simple; /api/status says why.
        None => Json(serde_json::json!({ "type": "FeatureCollection", "features": [] })).into_response(),
    }
}

pub async fn airports(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    respond(state.datasets().airports_body.as_ref(), &headers)
}

pub async fn seaports(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    respond(state.datasets().seaports_body.as_ref(), &headers)
}

pub async fn reactors(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    respond(state.datasets().plants_body.as_ref(), &headers)
}
