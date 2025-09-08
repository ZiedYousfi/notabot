#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

//! Notabot — a modular, extensible wrapper around the Enigo library for declarative UI automation.
//!
//! This crate organizes the codebase into cohesive modules and exposes a convenient prelude
//! for downstream crates/binaries. Most implementation details live under the internal modules:
//! - `config`: Configuration models, loader, and schema helpers.
//! - `executor`: Action definitions and runtime execution engine.
//! - `sources`: Event sources (file, directory, TCP, stdin).
//! - `utils`: Utilities such as interpolation and (optional) window helpers.
//!
//! Use `notabot::prelude::*` to bring commonly used items into scope quickly.

/// Public module: configuration (models, loader, schema helpers).
pub mod config;
/// Public module: execution engine (actions and runtime).
pub mod executor;
/// Public module: event sources (file, directory, tcp, stdin).
pub mod sources;
/// Public module: utilities (interpolation, window helpers, etc.).
pub mod utils;

/// Crate-level constants for consumers that want to inspect package metadata at runtime.
pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");
pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the crate version (e.g., "0.1.0").
#[inline]
pub const fn version() -> &'static str {
    PKG_VERSION
}

/// Initialize tracing (logging) with a reasonable default.
/// - Honors the `RUST_LOG` environment variable if set.
/// - Falls back to `info` level.
///
/// Safe to call multiple times; subsequent calls are no-ops.
pub fn init_tracing() {
    use tracing::Level;
    use tracing_subscriber::fmt;

    // Parse RUST_LOG as a simple level (trace|debug|info|warn|error)
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "trace" => Some(Level::TRACE),
            "debug" => Some(Level::DEBUG),
            "info" => Some(Level::INFO),
            "warn" | "warning" => Some(Level::WARN),
            "error" => Some(Level::ERROR),
            _ => None,
        })
        .unwrap_or(Level::INFO);

    // Ignore the error if the global subscriber was already set.
    let _ = fmt().with_max_level(level).try_init();
}

/// A convenient set of exports for most consumers.
///
/// Bring this into scope with:
/// `use notabot::prelude::*;`
pub mod prelude {
    // Common result/error handling
    pub use anyhow::{Context, Error, Result, anyhow, bail, ensure};

    // Serialization
    pub use serde::{Deserialize, Serialize};

    // Tracing macros
    pub use tracing::{debug, error, info, instrument, trace, warn};

    // Timing helpers
    pub use std::time::Duration;
    pub use tokio::time::sleep;

    // External crates (namespaced) if callers want direct access
    pub use crate as notabot;
    pub use enigo;
    pub use rand;

    // Frequently used internal modules
    pub use crate::{config, executor, sources, utils};
}
