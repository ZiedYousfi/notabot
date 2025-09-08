use std::path::PathBuf;

use clap::Parser;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use notabot::config as cfg;
use notabot::executor::Runtime;
use notabot::sources;

/// Notabot CLI
#[derive(Debug, Parser)]
#[command(
    name = notabot::PKG_NAME,
    version = notabot::PKG_VERSION,
    about = "A modular, extensible wrapper around Enigo for declarative UI automation"
)]
struct Args {
    /// Path to the JSON configuration file
    #[arg(short = 'c', long = "config", default_value = "config/default.json")]
    config: PathBuf,

    /// Enable dry-run mode (log actions instead of simulating input)
    #[arg(long = "dry-run")]
    dry_run: bool,

    /// Set log level (e.g., trace, debug, info, warn, error). Overrides RUST_LOG.
    #[arg(long = "log-level")]
    log_level: Option<String>,

    /// Print the JSON Schema for the configuration and exit
    #[arg(long = "print-schema")]
    print_schema: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Honor --log-level by setting RUST_LOG before initializing tracing.
    if let Some(level) = &args.log_level {
        let level = match level.to_lowercase().as_str() {
            "trace" => tracing::Level::TRACE,
            "debug" => tracing::Level::DEBUG,
            "info" => tracing::Level::INFO,
            "warn" | "warning" => tracing::Level::WARN,
            "error" => tracing::Level::ERROR,
            _ => tracing::Level::INFO,
        };
        let _ = tracing_subscriber::fmt().with_max_level(level).try_init();
    }

    if args.log_level.is_none() {
        notabot::init_tracing();
    }
    info!(
        version = notabot::PKG_VERSION,
        config = %args.config.display(),
        dry_run = args.dry_run,
        "Starting Notabot"
    );

    if args.print_schema {
        let schema = cfg::generate_schema();
        let json = serde_json::to_string_pretty(&schema)?;
        println!("{json}");
        return Ok(());
    }

    // Load configuration
    let config = cfg::load_from_path_async(&args.config).await?;
    debug!(target: "notabot", "Configuration loaded successfully");

    // Create the runtime (owns the config)
    let mut runtime = Runtime::new(config, args.dry_run);

    // Build and spawn event sources based on config
    let sources = sources::build_sources_from_config(runtime.config());
    if sources.is_empty() {
        warn!("No event sources configured. The runtime will wait for Ctrl+C and then exit.");
    }

    // Channel for events produced by sources
    let (tx, mut rx) = mpsc::channel::<Value>(256);
    let _handles = sources::spawn_all_sources(&sources, tx);

    // Main loop: handle events or Ctrl+C
    tokio::select! {
        _ = async {
            while let Some(event) = rx.recv().await {
                match runtime.run_event(&event) {
                    Ok(()) => { /* ok */ }
                    Err(err) => {
                        error!(error = %err, event = %event, "Failed to handle event");
                    }
                }
            }
        } => {}
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down");
        }
    }

    info!("Notabot exited");
    Ok(())
}
