/*!
Event sources module (orchestration layer).

This module only defines the core `EventSource` trait and orchestration helpers
(`build_sources_from_config`, `spawn_all_sources`). Concrete implementations now
live in their own files:

- `file.rs`      -> `FileSource`     (poll a single JSON file)
- `directory.rs` -> `DirectorySource` (poll / (future) watch a directory of JSON files)
- `tcp.rs`       -> `TcpSource`      (newline-delimited JSON over TCP)
- `stdin_source.rs` -> `StdinSource`    (newline-delimited JSON from standard input)

Each source implementation is responsible for:
- Parsing raw input into `serde_json::Value`
- Pushing events via `Sender<Value>` while respecting backpressure (`send().await`)
- Logging errors and continuing (never panicking inside tasks)
- Being cancellation-safe (task ends cleanly when channel closes / loop breaks)

Adding a new source:
1. Create `src/sources/your_source.rs`
2. Implement a `YourSource` struct + `impl EventSource`
3. Expose with `pub use self::your_source::YourSource;`
4. Extend `build_sources_from_config` match on `SourceConfig`

This keeps `mod.rs` lean and focused on wiring, making each source simpler to
maintain and test in isolation.
*/

use serde_json::Value;
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tracing::info;

use crate::config::{Config, SourceConfig};

pub mod directory;
pub mod file;
pub mod stdin_source;
pub mod tcp;

pub use directory::DirectorySource;
pub use file::FileSource;
pub use stdin_source::StdinSource;
pub use tcp::TcpSource;

/// Trait implemented by all event sources.
///
/// A source is expected to spawn an asynchronous task that produces JSON events
/// and sends them into the provided channel. Tasks should never panic; log and
/// continue or exit gracefully on unrecoverable errors.
pub trait EventSource: Send + Sync {
    /// Static human-readable identifier (used in logs).
    fn name(&self) -> &'static str;

    /// Start the source in the background.
    ///
    /// Implementations typically:
    /// - clone internal state
    /// - spawn a `tokio::task`
    /// - loop, producing events
    /// - exit when channel is closed or unrecoverable error occurs
    fn start(&self, sender: Sender<Value>) -> JoinHandle<()>;
}

/// Construct all configured sources.
///
/// Notes:
/// - If a `stdin` source is present it will be added (only one usually makes sense).
/// - Order of sources in the returned vector is the same as in the config.
pub fn build_sources_from_config(cfg: &Config) -> Vec<Box<dyn EventSource>> {
    let mut out: Vec<Box<dyn EventSource>> = Vec::new();

    for sc in &cfg.sources {
        match sc {
            SourceConfig::File {
                path,
                poll_ms,
                delete_on_success,
            } => out.push(Box::new(FileSource::new(
                path.clone(),
                *poll_ms,
                *delete_on_success,
            ))),

            SourceConfig::Directory {
                path,
                pattern,
                recursive,
            } => out.push(Box::new(DirectorySource::new(
                path.clone(),
                pattern.clone(),
                recursive.unwrap_or(false),
            ))),

            SourceConfig::Tcp { bind, ack } => {
                out.push(Box::new(TcpSource::new(bind.clone(), ack.unwrap_or(true))));
            }

            SourceConfig::Stdin => {
                out.push(Box::new(StdinSource::new()));
            }
        }
    }

    out
}

/// Spawn every source, returning their `JoinHandle`s.
///
/// The caller may store these if it wishes to monitor or await their termination.
/// Typically the application just keeps them detached and relies on process
/// lifetime / Ctrl+C for shutdown.
pub fn spawn_all_sources(
    sources: &[Box<dyn EventSource>],
    sender: Sender<Value>,
) -> Vec<JoinHandle<()>> {
    sources
        .iter()
        .map(|src| {
            info!(
                target: "notabot::sources",
                source = %src.name(),
                "Starting source task"
            );
            src.start(sender.clone())
        })
        .collect()
}
