use clap::{Parser, Subcommand};

/// WorldMap server and tile builder.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the API server (default when no command is given).
    Serve,
    /// Build vector tiles (MBTiles) from GeoJSON, GeoJSONSeq or GeoPackage files.
    Tiles(Box<worldmap::tilegen::cli::TilesArgs>),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => {
            let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            runtime.block_on(worldmap::server::run())
        }
        Command::Tiles(args) => {
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
                .init();
            worldmap::tilegen::cli::run(*args)
        }
    }
}
