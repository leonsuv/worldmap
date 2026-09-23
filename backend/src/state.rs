use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::ais::AisHub;
use crate::config::Config;
use crate::datasets::{db_mtime, Datasets};
use crate::db::DbPool;
use crate::routes::tiles::TileIndex;
use crate::upstream::Provider;

pub struct Providers {
    pub opensky: Provider,
    pub open_meteo: Provider,
    pub tomtom: Provider,
    pub nominatim: Provider,
    pub overpass: Provider,
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            opensky: Provider::new("OpenSky", 60, 1800),
            open_meteo: Provider::new("Open-Meteo", 60, 900),
            tomtom: Provider::new("TomTom", 60, 900),
            nominatim: Provider::new("Nominatim", 5, 300),
            overpass: Provider::new("Overpass", 30, 600),
        }
    }
}

pub struct AppState {
    pub config: Config,
    pub cache_db: DbPool,
    pub static_db_path: PathBuf,
    datasets: RwLock<Arc<Datasets>>,
    pub tiles: TileIndex,
    pub ais: Arc<AisHub>,
    pub http: reqwest::Client,
    /// Cached OpenSky OAuth token and its expiry (Unix seconds).
    pub opensky_token: tokio::sync::Mutex<Option<(String, i64)>>,
    pub providers: Providers,
    pub started_at: i64,
}

impl AppState {
    pub fn new(config: Config, cache_db: DbPool, http: reqwest::Client) -> Self {
        let static_db_path = config.data_dir.join("static.db");
        let datasets = Datasets::load(&static_db_path);
        let tiles = TileIndex::new(config.tiles_dir());
        let ais = AisHub::new(config.aisstream_key.is_some());
        Self {
            config,
            cache_db,
            static_db_path,
            datasets: RwLock::new(Arc::new(datasets)),
            tiles,
            ais,
            http,
            opensky_token: tokio::sync::Mutex::new(None),
            providers: Providers::default(),
            started_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn datasets(&self) -> Arc<Datasets> {
        self.datasets.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reload imported datasets when `static.db` changed on disk.
    pub fn reload_datasets_if_changed(&self) -> bool {
        let current = self.datasets().loaded_from;
        let on_disk = db_mtime(&self.static_db_path);
        if on_disk.is_none() || on_disk == current {
            return false;
        }
        let fresh = Datasets::load(&self.static_db_path);
        *self.datasets.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(fresh);
        tracing::info!("Reloaded datasets from {}", self.static_db_path.display());
        true
    }
}
