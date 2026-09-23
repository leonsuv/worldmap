//! MBTiles 1.3 writer. Output goes to a temporary file that replaces the
//! target only after a successful build.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

pub struct Writer {
    conn: Connection,
    temp: PathBuf,
    target: PathBuf,
}

impl Writer {
    pub fn create(target: &Path) -> Result<Self> {
        if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let temp = target.with_extension("mbtiles.part");
        let _ = std::fs::remove_file(&temp);
        let conn = Connection::open(&temp).with_context(|| format!("creating {}", temp.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode = OFF;
             PRAGMA synchronous = OFF;
             PRAGMA page_size = 4096;
             CREATE TABLE metadata (name TEXT, value TEXT);
             CREATE UNIQUE INDEX name ON metadata (name);
             CREATE TABLE tiles (zoom_level INTEGER, tile_column INTEGER, tile_row INTEGER, tile_data BLOB);
             CREATE UNIQUE INDEX tile_index ON tiles (zoom_level, tile_column, tile_row);",
        )?;
        Ok(Self { conn, temp, target: target.to_path_buf() })
    }

    /// Insert tiles given in XYZ scheme (rows are flipped to TMS here).
    pub fn write_tiles(&mut self, z: u8, tiles: &[(u32, u32, Vec<u8>)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare_cached("INSERT OR REPLACE INTO tiles (zoom_level, tile_column, tile_row, tile_data) VALUES (?1, ?2, ?3, ?4)")?;
            let flip = (1u32 << z) - 1;
            for (x, y, data) in tiles {
                stmt.execute(params![z, x, flip - y, data])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_metadata(&mut self, entries: &[(&str, String)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for (k, v) in entries {
            tx.execute("INSERT OR REPLACE INTO metadata (name, value) VALUES (?1, ?2)", params![k, v])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Close the database and move it into place.
    pub fn finish(self) -> Result<()> {
        let Writer { conn, temp, target } = self;
        conn.close().map_err(|(_, e)| e)?;
        // Windows cannot replace a file another process holds open; retry briefly.
        let mut last_error = None;
        for _ in 0..40 {
            match std::fs::rename(&temp, &target) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if target.exists() && std::fs::remove_file(&target).is_ok() {
                        continue;
                    }
                    last_error = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        }
        Err(anyhow::anyhow!(
            "could not replace {} ({}). Close programs using it and move {} there manually.",
            target.display(),
            last_error.map(|e| e.to_string()).unwrap_or_default(),
            temp.display()
        ))
    }
}
