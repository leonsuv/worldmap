//! HTTP server setup, background tasks and graceful shutdown.

use axum::http::{HeaderValue, Method};
use axum::response::Html;
use axum::routing::{any, delete, get, post};
use axum::Router;
use std::sync::Arc;
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::ais::{self, MAX_POSITION_AGE_SECS};
use crate::config::Config;
use crate::db::DbPool;
use crate::routes::{
    self, alerts, events, export, flights, history, search, ships, static_data, status, tiles, traffic, watchlist, weather,
};
use crate::state::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/status", get(status::status))
        .route("/api/search", get(search::search))
        .route("/api/flights", get(flights::get_flights))
        .route("/api/flights/track", get(flights::get_track))
        .route("/api/flights/aircraft", get(flights::get_aircraft_flights))
        .route("/api/flights/airport", get(flights::get_airport_flights))
        .route("/api/ships/ws", get(ships::ws_handler))
        .route("/api/ships/snapshot", get(ships::snapshot))
        .route("/api/ships/aton", get(ships::aton))
        .route("/api/ships/{mmsi}", get(ships::details))
        .route("/api/weather", get(weather::get_point))
        .route("/api/weather/grid", get(weather::get_grid))
        .route("/api/traffic/{z}/{x}/{y}", get(traffic::get_tile))
        .route("/api/airports", get(static_data::airports))
        .route("/api/seaports", get(static_data::seaports))
        .route("/api/reactors", get(static_data::reactors))
        .route("/api/watchlist", get(watchlist::list).post(watchlist::create))
        .route("/api/watchlist/{id}", delete(watchlist::delete))
        .route("/api/events", get(events::list).post(events::create))
        .route("/api/events/affected", get(events::affected))
        .route("/api/events/{id}", delete(events::delete))
        .route("/api/events/{id}/close", post(events::close))
        .route("/api/alerts", get(alerts::list))
        .route("/api/alerts/count", get(alerts::count))
        .route("/api/alerts/ack-all", post(alerts::acknowledge_all))
        .route("/api/alerts/{id}/ack", post(alerts::acknowledge))
        .route("/api/history/timestamps", get(history::timestamps))
        .route("/api/history/ships", get(history::ships))
        .route("/api/history/track", get(history::track))
        .route("/api/export/csv", get(export::export_csv))
        .route("/api/export/report", get(export::situation_report))
        .route("/api/{*rest}", any(routes::api_not_found))
        .route("/tiles/{source}/tilejson.json", get(tiles::tilejson))
        .route("/tiles/{source}/{z}/{x}/{y}", get(tiles::get_tile));

    let mut app = api.with_state(state.clone()).layer(CompressionLayer::new()).layer(TraceLayer::new_for_http());

    if !state.config.cors_origins.is_empty() {
        let origins: Vec<HeaderValue> = state.config.cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
        app = app.layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([Method::GET, Method::POST, Method::DELETE])
                .allow_headers([axum::http::header::CONTENT_TYPE]),
        );
    }

    match &state.config.frontend_dir {
        Some(dir) => app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))),
        None => app.fallback(|| async {
            Html(
                "<!doctype html><title>WorldMap API</title><body style=\"font-family:system-ui;padding:2rem\">\
                 <h1>WorldMap API is running</h1><p>No built frontend was found. Run <code>npm run build</code> in \
                 <code>frontend/</code>, or open the Vite dev server at <a href=\"http://localhost:5173\">localhost:5173</a>.</p>",
            )
        }),
    }
}

fn spawn_background_tasks(state: Arc<AppState>) {
    // Publish changed vessels once per second.
    {
        let hub = state.ais.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                if let Some(batch) = hub.flush_batch() {
                    let _ = hub.updates.send(batch);
                }
            }
        });
    }

    // Every 30 s: prune, persist live vessels, pick up new tiles and datasets.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            tick.tick().await;
            loop {
                tick.tick().await;
                let now = chrono::Utc::now().timestamp();
                state.ais.prune(now);
                let ships = state.ais.ships_updated_since(now - 45);
                if !ships.is_empty() {
                    let db = state.cache_db.clone();
                    if let Ok(Err(e)) = tokio::task::spawn_blocking(move || db.save_ships(&ships)).await {
                        tracing::warn!("Persisting vessels failed: {e:#}");
                    }
                }
                let s = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    s.tiles.rescan();
                    s.reload_datasets_if_changed();
                })
                .await;
            }
        });
    }

    // Every 5 min: history snapshot; hourly: housekeeping.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(300));
            tick.tick().await;
            let mut rounds: u64 = 0;
            loop {
                tick.tick().await;
                rounds += 1;
                let now = chrono::Utc::now().timestamp();
                // Only vessels with a recent report: stale positions are not history.
                let ships = state.ais.ships_updated_since(now - 600);
                let db = state.cache_db.clone();
                let hourly = rounds.is_multiple_of(12);
                let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                    if !ships.is_empty() {
                        db.save_ship_history(&ships, now)?;
                    }
                    db.prune_ship_history(3 * 86400)?;
                    if hourly {
                        db.cache_prune(86400)?;
                        db.delete_ships_older_than(86400)?;
                    }
                    Ok(())
                })
                .await;
                if let Ok(Err(e)) = result {
                    tracing::warn!("History maintenance failed: {e:#}");
                }
            }
        });
    }

    // Every minute: raise alerts for watched vessels that entered an event area.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            tick.tick().await;
            loop {
                tick.tick().await;
                alerts::evaluate(&state, None, None).await;
            }
        });
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutting down…");
}

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=warn".into()))
        .init();
    config.log_summary();

    std::fs::create_dir_all(config.tiles_dir())?;
    let cache_db = DbPool::open_cache(config.data_dir.join("cache.db"))?;
    let http = reqwest::Client::builder()
        .user_agent(concat!("WorldMap/", env!("CARGO_PKG_VERSION"), " (+https://github.com/leonsuv/worldmap)"))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()?;
    let state = Arc::new(AppState::new(config.clone(), cache_db.clone(), http));

    match cache_db.load_ships(MAX_POSITION_AGE_SECS) {
        Ok(ships) => {
            tracing::info!("Restored {} recent vessel positions", ships.len());
            state.ais.seed(ships);
        }
        Err(e) => tracing::warn!("Could not restore vessels: {e:#}"),
    }
    if let Some(key) = config.aisstream_key.clone() {
        ais::stream::spawn(key, state.ais.clone());
    }
    spawn_background_tasks(state.clone());

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("cannot listen on {}: {e} (is another WorldMap server running?)", config.bind_addr))?;
    tracing::info!("WorldMap listening on http://{}", config.bind_addr);
    axum::serve(listener, router(state.clone())).with_graceful_shutdown(shutdown_signal()).await?;

    let ships = state.ais.all_ships();
    if !ships.is_empty() {
        let db = state.cache_db.clone();
        let _ = tokio::task::spawn_blocking(move || db.save_ships(&ships)).await;
        tracing::info!("Saved live vessels");
    }
    Ok(())
}
