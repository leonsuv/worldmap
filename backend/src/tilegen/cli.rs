//! `worldmap-backend tiles …` command line.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use rusqlite::{Connection, OpenFlags};
use std::io::Read;
use std::path::PathBuf;

use super::{build, input, mvt, BuildOptions};

#[derive(Args)]
pub struct TilesArgs {
    #[command(subcommand)]
    command: TilesCommand,
}

#[derive(Subcommand)]
enum TilesCommand {
    /// Build an MBTiles vector tileset.
    Build(Box<BuildArgs>),
    /// Show metadata and per-zoom statistics of an MBTiles file.
    Inspect {
        path: PathBuf,
        /// Also decode one tile, e.g. `6/34/21`.
        #[arg(long)]
        tile: Option<String>,
    },
    /// List the feature tables of a GeoPackage.
    Tables { path: PathBuf },
}

#[derive(Args)]
struct BuildArgs {
    /// Output .mbtiles file.
    #[arg(short, long)]
    output: PathBuf,
    /// Input files (.geojson, .geojsonl/.geojsonseq/.ndjson, .gpkg); repeatable.
    #[arg(short, long, required = true, num_args = 1..)]
    input: Vec<PathBuf>,
    /// Vector layer name inside the tiles.
    #[arg(short, long, default_value = "features")]
    layer: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    attribution: Option<String>,
    #[arg(short = 'Z', long, default_value_t = 0)]
    minzoom: u8,
    #[arg(short = 'z', long, default_value_t = 12)]
    maxzoom: u8,
    /// Simplification tolerance in tile units (4096 per tile) below max zoom.
    #[arg(long, default_value_t = 1.0)]
    simplify: f64,
    /// Clip buffer in tile units.
    #[arg(long, default_value_t = 64)]
    buffer: u32,
    /// Omit lines shorter than this many pixels below max zoom (0 = keep all).
    #[arg(long, default_value_t = 0.0)]
    min_length: f64,
    /// Compressed size limit per tile in KiB.
    #[arg(long, default_value_t = 500)]
    max_tile_kb: usize,
    /// Keep features with identical attributes separate.
    #[arg(long)]
    no_coalesce: bool,
    /// Attributes to keep (comma-separated; default all).
    #[arg(long, value_delimiter = ',')]
    include: Vec<String>,
    /// Attributes to drop (comma-separated).
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,
    /// GeoPackage feature table to read.
    #[arg(long)]
    gpkg_table: Option<String>,
    /// SQL condition for GeoPackage rows.
    #[arg(long = "where")]
    where_clause: Option<String>,
}

pub fn run(args: TilesArgs) -> Result<()> {
    match args.command {
        TilesCommand::Build(a) => {
            let a = *a;
            let mut opts = BuildOptions::new(a.output, a.input, &a.layer);
            if let Some(name) = a.name {
                opts.name = name;
            }
            opts.description = a.description;
            opts.attribution = a.attribution;
            opts.minzoom = a.minzoom;
            opts.maxzoom = a.maxzoom;
            opts.simplify = a.simplify.max(0.0);
            opts.buffer = a.buffer.min(1024);
            opts.min_length_px = a.min_length.max(0.0);
            opts.max_tile_bytes = a.max_tile_kb.max(16) * 1024;
            opts.coalesce = !a.no_coalesce;
            opts.input_options.include = (!a.include.is_empty()).then(|| a.include.into_iter().collect());
            opts.input_options.exclude = a.exclude.into_iter().collect();
            opts.input_options.gpkg_table = a.gpkg_table;
            opts.input_options.where_clause = a.where_clause;
            build(&opts)?;
            Ok(())
        }
        TilesCommand::Inspect { path, tile } => inspect(&path, tile.as_deref()),
        TilesCommand::Tables { path } => {
            for (name, geometry, srs) in input::gpkg_tables(&path)? {
                println!("{name}\t{geometry}\tsrs_id={srs}");
            }
            Ok(())
        }
    }
}

fn inspect(path: &std::path::Path, tile: Option<&str>) -> Result<()> {
    let conn =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).with_context(|| format!("opening {}", path.display()))?;
    println!("{}", path.display());
    let mut stmt = conn.prepare("SELECT name, value FROM metadata ORDER BY name")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.flatten() {
        let value = if row.1.len() > 160 {
            format!("{}…", &row.1[..row.1.char_indices().nth(160).map(|c| c.0).unwrap_or(row.1.len())])
        } else {
            row.1
        };
        println!("  {:<12} {}", row.0, value);
    }
    println!("  zoom    tiles        size    largest");
    let mut stmt = conn.prepare(
        "SELECT zoom_level, COUNT(*), SUM(LENGTH(tile_data)), MAX(LENGTH(tile_data)) FROM tiles GROUP BY zoom_level ORDER BY zoom_level",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))?.flatten() {
        println!("  {:>4} {:>8} {:>9.1} KB {:>7.1} KB", row.0, row.1, row.2 as f64 / 1024.0, row.3 as f64 / 1024.0);
    }
    if let Some(spec) = tile {
        let parts: Vec<u32> = spec.split('/').filter_map(|s| s.parse().ok()).collect();
        let [z, x, y] = parts[..] else { anyhow::bail!("--tile must look like z/x/y") };
        let data: Vec<u8> = conn
            .query_row(
                "SELECT tile_data FROM tiles WHERE zoom_level=?1 AND tile_column=?2 AND tile_row=?3",
                rusqlite::params![z, x, (1u32 << z) - 1 - y],
                |r| r.get(0),
            )
            .with_context(|| format!("tile {spec} not found"))?;
        let raw = if data.starts_with(&[0x1f, 0x8b]) {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&data[..]).read_to_end(&mut out)?;
            out
        } else {
            data
        };
        for layer in mvt::decode_tile(&raw)? {
            let vertices: usize = layer.features.iter().map(|f| f.parts.iter().map(Vec::len).sum::<usize>()).sum();
            println!("  tile {spec}: layer '{}' — {} features, {} vertices", layer.name, layer.features.len(), vertices);
            for f in layer.features.iter().take(5) {
                println!("    type {} parts {} {:?}", f.geom_type, f.parts.len(), f.properties);
            }
        }
    }
    Ok(())
}
