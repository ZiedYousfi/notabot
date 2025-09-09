#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

/*!
Executor module for Notabot.

This module wires together:
- `actions`: low-level input simulation and helpers (mouse, keyboard, sleep, logging, window ops)
- `runtime`: high-level workflow execution with interpolation and variable mapping

Typical usage:
- Construct a `Runtime` with a loaded `Config`.
- Call `Runtime::run_event` with an incoming JSON event, or `run_workflow_by_name` directly.

Example:
```no_run
use notabot::config::{self, Config};
use notabot::executor::Runtime;
use serde_json::json;

// Load config (omitted) then:
let cfg: Config = Default::default();
let mut rt = Runtime::new(cfg, true); // dry-run mode
let event = json!({"type": "send_text", "text": "Hello"});
// rt.run_event(&event)?;
```

Public re-exports:
- `ActionExecutor`: performs low-level actions (respecting dry-run).
- `Runtime`: orchestrates workflows and executes actions.
*/

pub mod actions;
pub mod runtime;

// Re-exports for convenient access from `notabot::executor::*`
pub use actions::ActionExecutor;
pub use runtime::Runtime;
