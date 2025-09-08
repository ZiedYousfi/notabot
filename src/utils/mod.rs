//! Utilities for Notabot.
//!
//! This module aggregates utility helpers used across the crate.
//!
//! Submodules:
//! - `interpolation`: Templating helpers for variables like `{{var}}` and globals `{{@key}}`.
//! - `window`: OS-specific window management helpers (no-op on unsupported platforms).

pub mod interpolation;
pub mod window;
