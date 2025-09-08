//! Configuration module for Notabot.
//!
//! This module wires together the data models and loading/validation helpers used
//! throughout the crate. Import from here for a convenient, stable API.
//!
//! Example:
//! use notabot::config::{Config, load_from_path};
//!
//! let cfg = load_from_path("config/default.json")?;

pub mod loader;
pub mod models;

// Re-export core data models
pub use models::{
    ActionDef, Config, EventBinding, EventMap, GlobalsMap, LogLevel, MouseButton, NamedActions,
    Rect, SourceConfig, VarsMap, Workflows,
};

// Re-export loader utilities
pub use loader::{
    generate_schema, load_from_path, load_from_path_async, load_from_reader, load_from_str,
    validate_config, write_schema_to_writer,
};
