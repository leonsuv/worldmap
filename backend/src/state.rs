use crate::db::DbPool;
use crate::routes::ships::{AtoNStore, SarStore, ShipStore};
use crate::routes::tiles::TileIndex;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct AppState {
    pub cache_db: DbPool,
    pub static_db: DbPool,
    pub tile_index: TileIndex,
    pub ship_broadcast: Arc<broadcast::Sender<String>>,
    pub ship_store: ShipStore,
    pub aton_store: AtoNStore,
    pub sar_store: SarStore,
    pub http_client: reqwest::Client,
    pub airports_geojson: serde_json::Value,
    pub seaports_geojson: serde_json::Value,
    /// OAuth2 client credentials (client_id, client_secret)
    pub opensky_creds: Option<(String, String)>,
    /// Cached bearer token (token, expires_at_unix)
    pub opensky_token: tokio::sync::Mutex<Option<(String, i64)>>,
}
