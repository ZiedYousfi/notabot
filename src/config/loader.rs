use anyhow::{Context, Result, bail};
use schemars::{Schema, schema_for};
use serde_json;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use tracing::{debug, trace};

use super::models::{ActionDef, Config};

/// Load configuration from a string slice.
pub fn load_from_str(s: &str) -> Result<Config> {
    let cfg: Config =
        serde_json::from_str(s).context("Failed to parse JSON config string into Config")?;
    validate_config(&cfg)?;
    Ok(cfg)
}

/// Load configuration from any reader (e.g., a file).
pub fn load_from_reader<R: Read>(reader: R) -> Result<Config> {
    let cfg: Config =
        serde_json::from_reader(reader).context("Failed to parse JSON config from reader")?;
    validate_config(&cfg)?;
    Ok(cfg)
}

/// Load configuration from a file path synchronously.
pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Config> {
    let path_ref = path.as_ref();
    let file = File::open(path_ref)
        .with_context(|| format!("Failed to open config file {}", path_ref.display()))?;
    let cfg = load_from_reader(file)?;
    debug!("Loaded config from {}", path_ref.display());
    Ok(cfg)
}

/// Load configuration from a file path asynchronously (Tokio).
pub async fn load_from_path_async<P: AsRef<Path>>(path: P) -> Result<Config> {
    use tokio::fs;
    let path_ref = path.as_ref();
    let bytes = fs::read(path_ref)
        .await
        .with_context(|| format!("Failed to read config file {}", path_ref.display()))?;
    let cfg: Config = serde_json::from_slice(&bytes)
        .with_context(|| format!("Failed to parse JSON config from {}", path_ref.display()))?;
    validate_config(&cfg)?;
    debug!("Loaded config from {}", path_ref.display());
    Ok(cfg)
}

/// Generate the JSON Schema for the Config model (for external validation or tooling).
pub fn generate_schema() -> Schema {
    schema_for!(Config)
}

/// Write the JSON Schema for the Config model to any writer (pretty-printed).
pub fn write_schema_to_writer<W: Write>(mut writer: W) -> Result<()> {
    let schema = generate_schema();
    let json = serde_json::to_string_pretty(&schema).context("Failed to serialize schema")?;
    writer
        .write_all(json.as_bytes())
        .context("Failed to write schema to writer")?;
    Ok(())
}

/// Placeholder for schema-based validation.
/// Currently a no-op. You can integrate a JSON Schema validator here if desired.
/// Returns Ok(()) if validation passes or is skipped.
pub fn validate_with_schema_placeholder(_config: &Config) -> Result<()> {
    // Example idea:
    // - Use `jsonschema` crate to compile `generate_schema()` and validate a serde_json::Value of the config.
    // - Or derive `serde_valid::Validate` on models and call `validate()` here.
    trace!("Schema validation placeholder (no-op)");
    Ok(())
}

/// Perform basic sanity checks and internal reference validation.
/// - Ensure events reference existing workflows.
/// - Ensure `Ref` actions reference existing named actions.
pub fn validate_config(cfg: &Config) -> Result<()> {
    // Ensure events reference existing workflows
    for (event_type, binding) in &cfg.events {
        if !cfg.workflows.contains_key(&binding.workflow) {
            bail!(
                "Event '{}' refers to missing workflow '{}'",
                event_type,
                binding.workflow
            );
        }
    }

    // Collect all names for fast lookup
    let named_action_names = cfg
        .actions
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    // Validate refs within named actions
    for (name, action) in &cfg.actions {
        validate_action_refs(action, &named_action_names)
            .with_context(|| format!("Invalid reference in named action '{}'", name))?;
    }

    // Validate refs within workflows
    for (wf_name, steps) in &cfg.workflows {
        for (idx, step) in steps.iter().enumerate() {
            validate_action_refs(step, &named_action_names).with_context(|| {
                format!(
                    "Invalid reference in workflow '{}' at step {}",
                    wf_name, idx
                )
            })?;
        }
    }

    // Optional schema step (currently a no-op)
    validate_with_schema_placeholder(cfg)?;

    Ok(())
}

fn validate_action_refs(
    action: &ActionDef,
    named_action_names: &std::collections::BTreeSet<String>,
) -> Result<()> {
    match action {
        ActionDef::Ref { name } => {
            if !named_action_names.contains(name) {
                bail!("Referenced action '{}' was not found in `actions`", name);
            }
        }
        ActionDef::Sequence { steps } => {
            for (i, step) in steps.iter().enumerate() {
                validate_action_refs(step, named_action_names)
                    .with_context(|| format!("Invalid reference in sequence at index {}", i))?;
            }
        }
        ActionDef::Conditional { then, else_, .. } => {
            validate_action_refs(then, named_action_names)
                .context("Invalid reference in conditional `then` branch")?;
            if let Some(else_action) = else_ {
                validate_action_refs(else_action, named_action_names)
                    .context("Invalid reference in conditional `else` branch")?;
            }
        }
        // Leaf actions: nothing to validate
        ActionDef::MouseMove { .. }
        | ActionDef::MouseClick { .. }
        | ActionDef::MouseScroll { .. }
        | ActionDef::KeySeq { .. }
        | ActionDef::TypeText { .. }
        | ActionDef::SleepMs { .. }
        | ActionDef::SleepRandMs { .. }
        | ActionDef::FocusWindow { .. }
        | ActionDef::SetVar { .. }
        | ActionDef::Log { .. }
        | ActionDef::OcrCheck { .. }
        | ActionDef::CaptureScreen { .. } => {}
    }
    Ok(())
}
