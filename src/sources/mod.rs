/*!
Event sources module.

This module defines a common `EventSource` trait and basic stubs for concrete sources
(File, Directory, TCP). Each source can be started and will emit JSON events through
a shared `tokio::sync::mpsc::Sender<serde_json::Value>`.

Notes:
- The implementations provided here are placeholders (no-op). They only log that
  they were started and then exit. Replace with real implementations as needed.
- Use `build_sources_from_config` to construct sources from the configuration file.
*/

use serde_json::Value;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::{Config, SourceConfig};

/// A running producer of JSON events.
///
/// Implementations should push parsed events into the provided `Sender<Value>`.
/// They should run asynchronously until completion or cancellation.
pub trait EventSource: Send + Sync {
    /// A human-readable name for this source (for logs/diagnostics).
    fn name(&self) -> &'static str;

    /// Start the source in the background and return a `JoinHandle`.
    ///
    /// Implementations should:
    /// - clone any necessary state
    /// - spawn a `tokio::task` that emits events via the channel
    /// - log useful startup information
    fn start(&self, sender: Sender<Value>) -> JoinHandle<()>;
}

/// Build boxed sources from the loaded `Config`.
///
/// Currently supports `file`, `directory`, and `tcp`. `stdin` is logged and skipped.
pub fn build_sources_from_config(cfg: &Config) -> Vec<Box<dyn EventSource>> {
    let mut out: Vec<Box<dyn EventSource>> = Vec::new();

    for sc in &cfg.sources {
        match sc {
            SourceConfig::File {
                path,
                poll_ms,
                delete_on_success,
            } => {
                out.push(Box::new(FileSource::new(
                    path.clone(),
                    *poll_ms,
                    *delete_on_success,
                )));
            }
            SourceConfig::Directory {
                path,
                pattern,
                recursive,
            } => {
                out.push(Box::new(DirectorySource::new(
                    path.clone(),
                    pattern.clone(),
                    recursive.unwrap_or(false),
                )));
            }
            SourceConfig::Tcp { bind, ack } => {
                out.push(Box::new(TcpSource::new(bind.clone(), ack.unwrap_or(true))));
            }
            SourceConfig::Stdin => {
                warn!(target: "notabot::sources", "stdin source is not implemented yet; skipping");
            }
        }
    }

    out
}

/// Spawn all sources, returning their `JoinHandle`s.
///
/// Each source receives a cloned `Sender<Value>`.
pub fn spawn_all_sources(
    sources: &[Box<dyn EventSource>],
    sender: Sender<Value>,
) -> Vec<JoinHandle<()>> {
    sources
        .iter()
        .map(|src| {
            let s = sender.clone();
            info!(target: "notabot::sources", source = %src.name(), "Starting source");
            src.start(s)
        })
        .collect()
}

/// File-based event source (stub).
///
/// Intended behavior (to implement):
/// - Poll a single file path at a fixed interval (ms)
/// - When content is present, parse as JSON and send into the channel
/// - Optionally delete the file on success
#[derive(Debug, Clone)]
pub struct FileSource {
    path: String,
    poll_ms: u64,
    delete_on_success: bool,
}

impl FileSource {
    pub fn new(path: String, poll_ms: Option<u64>, delete_on_success: Option<bool>) -> Self {
        Self {
            path,
            poll_ms: poll_ms.unwrap_or(100),
            delete_on_success: delete_on_success.unwrap_or(false),
        }
    }
}

impl EventSource for FileSource {
    fn name(&self) -> &'static str {
        "file"
    }

    fn start(&self, _sender: Sender<Value>) -> JoinHandle<()> {
        let path = self.path.clone();
        let poll_ms = self.poll_ms;
        let del = self.delete_on_success;
        tokio::spawn(async move {
            warn!(
                target: "notabot::sources",
                %path, poll_ms, delete_on_success = del,
                "FileSource is not implemented yet; exiting stub task"
            );
        })
    }
}

/// Directory-based event source (stub).
///
/// Intended behavior (to implement):
/// - Watch a directory for new files (optionally filtered by pattern)
/// - Read/parse each file as JSON and send into the channel
/// - Consider FIFO processing and safe delete/move on success
#[derive(Debug, Clone)]
pub struct DirectorySource {
    path: String,
    pattern: Option<String>,
    recursive: bool,
}

impl DirectorySource {
    pub fn new(path: String, pattern: Option<String>, recursive: bool) -> Self {
        Self {
            path,
            pattern,
            recursive,
        }
    }
}

impl EventSource for DirectorySource {
    fn name(&self) -> &'static str {
        "directory"
    }

    fn start(&self, _sender: Sender<Value>) -> JoinHandle<()> {
        let path = self.path.clone();
        let pattern = self.pattern.clone();
        let recursive = self.recursive;
        tokio::spawn(async move {
            warn!(
                target: "notabot::sources",
                %path, ?pattern, recursive,
                "DirectorySource is not implemented yet; exiting stub task"
            );
        })
    }
}

/// TCP-based event source (stub).
///
/// Intended behavior (to implement):
/// - Bind to the given address (e.g., 127.0.0.1:5000)
/// - Accept connections and parse newline-delimited JSON events
/// - Optionally write ACK/ERROR responses
#[derive(Debug, Clone)]
pub struct TcpSource {
    bind: String,
    ack: bool,
}

impl TcpSource {
    pub fn new(bind: String, ack: bool) -> Self {
        Self { bind, ack }
    }
}

impl EventSource for TcpSource {
    fn name(&self) -> &'static str {
        "tcp"
    }

    fn start(&self, _sender: Sender<Value>) -> JoinHandle<()> {
        let bind = self.bind.clone();
        let ack = self.ack;
        tokio::spawn(async move {
            warn!(
                target: "notabot::sources",
                %bind, ack,
                "TcpSource is not implemented yet; exiting stub task"
            );
        })
    }
}
