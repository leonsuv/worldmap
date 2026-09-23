//! GET /api/status — what is configured and available, for the setup UI.

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use std::sync::Arc;

use crate::state::AppState;

pub async fn status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Cheap enough per request, and it makes "Recheck" pick up new files at once.
    let tiles_state = state.clone();
    let _ = tokio::task::spawn_blocking(move || {
        tiles_state.tiles.rescan();
        tiles_state.reload_datasets_if_changed();
    })
    .await;

    let ds = state.datasets();
    let ais = state.ais.status();
    let body = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "started_at": state.started_at,
        "tiles": state.tiles.names(),
        "tile_sources": state.tiles.summaries(),
        "ships_configured": state.config.aisstream_key.is_some(),
        "traffic_configured": state.config.tomtom_key.is_some(),
        "flights_authenticated": state.config.opensky_credentials.is_some(),
        "flights_refresh_secs": super::flights::states_ttl(state.config.opensky_credentials.is_some()),
        "ais": {
            "connected": ais.connected,
            "connected_since": ais.connected_since,
            "last_message_at": ais.last_message_at,
            "messages": ais.messages,
            "error": ais.last_error,
            "vessels": state.ais.ships.len(),
            "aids": state.ais.aton.len(),
        },
        "datasets": {
            "airports": ds.airports.len(),
            "seaports": ds.seaports.len(),
            "reactors": ds.plants.len(),
        },
    });
    ([(header::CACHE_CONTROL, "no-store")], Json(body))
}
