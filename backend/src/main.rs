mod cache_proxy;
mod db;
mod routes;
mod state;
mod ws_fanout;

use axum::{routing::get, Router};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

use crate::db::DbPool;
use crate::routes::tiles::TileIndex;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // Validate environment configuration
    validate_env();

    // Open databases
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let cache_db = DbPool::open_cache(format!("{data_dir}/cache.db"))?;
    let static_db = DbPool::open_static(format!("{data_dir}/static.db"))?;

    // Load tile sources
    let tiles_dir = std::path::Path::new(&data_dir).join("tiles");
    let tile_index = TileIndex::load_from_dir(&tiles_dir)?;
    tracing::info!("Tile sources loaded: {:?}", tile_index.source_names());

    // Ship broadcast channel
    let (ship_tx, _) = broadcast::channel::<String>(4096);
    let ship_tx = Arc::new(ship_tx);
    let ship_store = Arc::new(DashMap::new());
    let aton_store = Arc::new(DashMap::new());
    let sar_store = Arc::new(DashMap::new());

    // Pre-seed ship store from cache DB (ships seen in the last 30 minutes)
    match cache_db.load_ships(1800) {
        Ok(rows) => {
            for (mmsi, lat, lon, course, speed, heading, name, ship_type, ts) in &rows {
                ship_store.insert(*mmsi, routes::ships::ShipPosition {
                    mmsi: *mmsi, lat: *lat, lon: *lon,
                    course: *course, speed: *speed, heading: *heading,
                    ship_name: name.clone(), ship_type: *ship_type, timestamp: *ts,
                    imo: None, callsign: None, destination: None, eta: None,
                    draught: None, length: None, beam: None, nav_status: None,
                    rate_of_turn: None,
                });
            }
            tracing::info!("Pre-seeded {} ships from cache", rows.len());
        }
        Err(e) => tracing::warn!("Failed to load cached ships: {e}"),
    }

    // Spawn AIS WebSocket fanout (only if API key is set)
    if let Ok(ais_key) = std::env::var("AISSTREAM_API_KEY") {
        ws_fanout::spawn_ais_fanout(
            ais_key, ship_tx.clone(), ship_store.clone(),
            aton_store.clone(), sar_store.clone(),
        );
        tracing::info!("AIS stream fanout started");
    } else {
        tracing::warn!("AISSTREAM_API_KEY not set — ship tracking disabled");
    }

    // Periodic ship persistence (flush to SQLite every 30s)
    {
        let store = ship_store.clone();
        let db = cache_db.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let batch: Vec<_> = store.iter().map(|e| {
                    let s = e.value();
                    (s.mmsi, s.lat, s.lon, s.course, s.speed, s.heading,
                     s.ship_name.clone(), s.ship_type, s.timestamp)
                }).collect();
                if !batch.is_empty() {
                    let refs: Vec<_> = batch.iter().map(|s| {
                        (s.0, s.1, s.2, s.3, s.4, s.5, s.6.as_str(), s.7, s.8)
                    }).collect();
                    if let Err(e) = db.save_ships(&refs) {
                        tracing::warn!("Failed to persist ships: {e}");
                    } else {
                        tracing::debug!("Persisted {} ships to cache", refs.len());
                    }
                }
            }
        });
    }

    // Periodic ship history snapshots (every 5 minutes for historical replay)
    {
        let store = ship_store.clone();
        let db = cache_db.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                let now = chrono::Utc::now().timestamp();
                let batch: Vec<_> = store.iter().map(|e| {
                    let s = e.value();
                    (s.mmsi, s.lat, s.lon,
                     s.course, s.speed, s.heading,
                     s.ship_name.clone(), s.ship_type, now)
                }).collect();
                if !batch.is_empty() {
                    let refs: Vec<_> = batch.iter().map(|s| {
                        (s.0, s.1, s.2, s.3, s.4, s.5, s.6.as_str(), s.7, s.8)
                    }).collect();
                    if let Err(e) = db.save_ship_history(&refs) {
                        tracing::warn!("Failed to save ship history: {e}");
                    } else {
                        tracing::debug!("Saved {} ship history entries", refs.len());
                    }
                }
                // Prune entries older than 3 days
                if let Err(e) = db.prune_ship_history(259_200) {
                    tracing::warn!("Failed to prune ship history: {e}");
                }
            }
        });
    }

    // Shared HTTP client
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // OpenSky OAuth2 client credentials (optional — authenticated accounts get 4000 credits/day)
    let opensky_creds = match (std::env::var("OPENSKY_CLIENT_ID"), std::env::var("OPENSKY_CLIENT_SECRET")) {
        (Ok(id), Ok(secret)) if !id.is_empty() && !secret.is_empty() => {
            tracing::info!("OpenSky authenticated mode enabled (OAuth2 client credentials)");
            Some((id, secret))
        }
        _ => {
            tracing::info!("OpenSky anonymous mode (set OPENSKY_CLIENT_ID + OPENSKY_CLIENT_SECRET for higher rate limits)");
            None
        }
    };

    // Load static datasets into memory at startup
    let airports_geojson = {
        let conn = static_db.conn();
        routes::static_data::load_airports(&conn)
    };
    let seaports_geojson = {
        let conn = static_db.conn();
        routes::static_data::load_seaports(&conn)
    };

    let state = Arc::new(AppState {
        cache_db,
        static_db,
        tile_index,
        ship_broadcast: ship_tx,
        ship_store,
        aton_store,
        sar_store,
        http_client,
        airports_geojson,
        seaports_geojson,
        opensky_creds,
        opensky_token: tokio::sync::Mutex::new(None),
    });

    // SPA fallback: serve frontend/dist with index.html fallback
    let frontend_dir = std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "frontend/dist".to_string());
    let spa_service = ServeDir::new(&frontend_dir)
        .not_found_service(ServeFile::new(format!("{frontend_dir}/index.html")));

    let app = Router::new()
        // Tile routes — no CompressionLayer; tiles are pre-compressed gzip in MBTiles
        .route("/tiles/{source}/tilejson.json", get(routes::tiles::tilejson))
        .route("/tiles/{source}/{z}/{x}/{y}", get(routes::tiles::get_tile))
        // API routes with compression
        .route("/api/flights", get(routes::flights::get_flights))
        .route("/api/flights/track", get(routes::flights::get_track))
        .route("/api/flights/arrivals", get(routes::flights::get_arrivals))
        .route("/api/flights/departures", get(routes::flights::get_departures))
        .route("/api/flights/aircraft", get(routes::flights::get_flights_by_aircraft))
        .route("/api/flights/interval", get(routes::flights::get_flights_interval))
        .route("/api/ships/ws", get(routes::ships::ws_handler))
        .route("/api/ships/snapshot", get(routes::ships::snapshot))
        .route("/api/ships/aton", get(routes::ships::aton_snapshot))
        .route("/api/ships/sar", get(routes::ships::sar_snapshot))
        .route("/api/weather", get(routes::weather::get_weather))
        .route("/api/reactors", get(routes::reactors::get_reactors))
        .route("/api/traffic", get(routes::traffic::get_traffic))
        .route("/api/airports", get(routes::static_data::get_airports))
        .route("/api/seaports", get(routes::static_data::get_seaports))
        // Watchlist
        .route("/api/watchlist", get(routes::watchlist::list_watchlist).post(routes::watchlist::create_watchlist_item))
        .route("/api/watchlist/{id}", axum::routing::delete(routes::watchlist::delete_watchlist_item))
        // Events
        .route("/api/events", get(routes::events::list_events).post(routes::events::create_event))
        .route("/api/events/{id}", axum::routing::delete(routes::events::delete_event))
        .route("/api/events/{id}/close", axum::routing::post(routes::events::close_event))
        .route("/api/events/affected", get(routes::events::get_affected))
        // Alerts
        .route("/api/alerts", get(routes::alerts::list_alerts))
        .route("/api/alerts/count", get(routes::alerts::alert_count))
        .route("/api/alerts/{id}/ack", axum::routing::post(routes::alerts::acknowledge_alert))
        .route("/api/alerts/ack-all", axum::routing::post(routes::alerts::acknowledge_all))
        // History
        .route("/api/history/ships", get(routes::history::get_ship_history))
        .route("/api/history/timestamps", get(routes::history::get_history_timestamps))
        // Export
        .route("/api/export/csv", get(routes::export::export_csv))
        .route("/api/export/report", get(routes::export::situation_report))
        .with_state(state)
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        // SPA fallback
        .fallback_service(spa_service);

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Server listening on {bind_addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

fn validate_env() {
    let checks: &[(&str, &str, bool)] = &[
        ("DATA_DIR",          "Path to data/ directory",                     false),
        ("FRONTEND_DIR",      "Path to frontend/dist/",                      false),
        ("AISSTREAM_API_KEY", "AIS ship tracking — https://aisstream.io",    true),
        ("TOMTOM_API_KEY",    "Traffic data — https://developer.tomtom.com", false),
        ("BIND_ADDR",         "Server bind address (default 0.0.0.0:3000)",  false),
    ];

    let mut missing_required = Vec::new();
    for &(name, desc, required) in checks {
        match std::env::var(name) {
            Ok(v) if !v.is_empty() => tracing::info!("  ✓ {name}"),
            _ if required => {
                tracing::warn!("  ✗ {name} — MISSING (required) — {desc}");
                missing_required.push(name);
            }
            _ => tracing::info!("  · {name} — not set (optional) — {desc}"),
        }
    }

    if !missing_required.is_empty() {
        tracing::warn!(
            "Some required env vars are missing: {}. Copy backend/.env.example → .env and fill them in.",
            missing_required.join(", ")
        );
    }
}
