//! Runtime configuration from the environment (and `.env` files).
//!
//! Paths resolve the same way whether the server is started from the
//! repository root, from `backend/`, via `cargo run`, or from an unpacked
//! release archive, so a fresh checkout works without editing paths.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub frontend_dir: Option<PathBuf>,
    pub bind_addr: String,
    pub aisstream_key: Option<String>,
    pub tomtom_key: Option<String>,
    pub opensky_credentials: Option<(String, String)>,
    pub cors_origins: Vec<String>,
}

impl Config {
    /// Load `.env` files, then read the configuration from the environment.
    pub fn load() -> Self {
        let root = project_root();
        // A `.env` in the working directory wins; otherwise use backend/.env in a
        // checkout, or the one next to the executable in a release folder.
        if dotenvy::dotenv().is_err() {
            let fallback = match &root {
                Some(root) => Some(root.join("backend").join(".env")),
                None => std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join(".env"))),
            };
            if let Some(path) = fallback {
                let _ = dotenvy::from_path(path);
            }
        }
        Self::from_env(root.as_deref())
    }

    fn from_env(root: Option<&Path>) -> Self {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));

        let data_dir = match env_value("DATA_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => match (root, &exe_dir) {
                (Some(root), _) => root.join("data"),
                (None, Some(exe)) => exe.join("data"),
                (None, None) => PathBuf::from("data"),
            },
        };

        let frontend_dir = match env_value("FRONTEND_DIR") {
            Some(dir) => Some(PathBuf::from(dir)),
            None => {
                let mut candidates = Vec::new();
                if let Some(root) = root {
                    candidates.push(root.join("frontend").join("dist"));
                }
                if let Some(exe) = &exe_dir {
                    candidates.push(exe.join("frontend-dist"));
                    candidates.push(exe.join("frontend"));
                }
                candidates.into_iter().find(|dir| dir.join("index.html").is_file())
            }
        };

        let opensky_credentials = match (env_value("OPENSKY_CLIENT_ID"), env_value("OPENSKY_CLIENT_SECRET")) {
            (Some(id), Some(secret)) => Some((id, secret)),
            _ => None,
        };

        Self {
            data_dir,
            frontend_dir,
            bind_addr: env_value("BIND_ADDR").unwrap_or_else(|| "127.0.0.1:3000".to_string()),
            aisstream_key: env_value("AISSTREAM_API_KEY"),
            tomtom_key: env_value("TOMTOM_API_KEY"),
            opensky_credentials,
            cors_origins: env_value("CORS_ALLOW_ORIGINS")
                .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default(),
        }
    }

    pub fn tiles_dir(&self) -> PathBuf {
        self.data_dir.join("tiles")
    }

    /// Log which optional integrations are active without printing secrets.
    pub fn log_summary(&self) {
        tracing::info!("Data directory: {}", self.data_dir.display());
        match &self.frontend_dir {
            Some(dir) => tracing::info!("Serving frontend from {}", dir.display()),
            None => {
                tracing::warn!("No built frontend found (run `npm run build` in frontend/). API only; use the Vite dev server on :5173.")
            }
        }
        let flag = |on: bool| if on { "enabled" } else { "disabled" };
        tracing::info!("Vessels & navigation aids (AISSTREAM_API_KEY): {}", flag(self.aisstream_key.is_some()));
        tracing::info!("Road traffic (TOMTOM_API_KEY): {}", flag(self.tomtom_key.is_some()));
        tracing::info!(
            "Flights: OpenSky {}",
            if self.opensky_credentials.is_some() { "authenticated" } else { "anonymous (low rate limit)" }
        );
    }
}

/// Environment value with placeholders such as `your_key_here` treated as unset.
fn env_value(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("your_key_here") || value.starts_with("your_") {
        return None;
    }
    Some(value.to_string())
}

/// Find the repository root (the directory containing `backend/` and `frontend/`).
fn project_root() -> Option<PathBuf> {
    let is_root = |dir: &Path| dir.join("backend").join("Cargo.toml").is_file() && dir.join("frontend").is_dir();
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        starts.push(exe);
    }
    starts.into_iter().find_map(|start| start.ancestors().find(|dir| is_root(dir)).map(Path::to_path_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_keys_are_treated_as_missing() {
        std::env::set_var("WORLDMAP_TEST_PLACEHOLDER", "your_key_here");
        assert_eq!(env_value("WORLDMAP_TEST_PLACEHOLDER"), None);
        std::env::set_var("WORLDMAP_TEST_PLACEHOLDER", "  ");
        assert_eq!(env_value("WORLDMAP_TEST_PLACEHOLDER"), None);
        std::env::set_var("WORLDMAP_TEST_PLACEHOLDER", " abc ");
        assert_eq!(env_value("WORLDMAP_TEST_PLACEHOLDER").as_deref(), Some("abc"));
    }

    #[test]
    fn finds_the_repository_root_from_the_backend_directory() {
        let root = project_root().expect("tests run inside the repository");
        assert!(root.join("backend").join("Cargo.toml").is_file());
    }
}
