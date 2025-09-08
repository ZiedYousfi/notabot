use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info, trace, warn};

use crate::config::{ActionDef, Config, EventBinding};
use crate::executor::actions::ActionExecutor;
use crate::utils::interpolation;

/// Maximum nesting depth for action execution (to protect against cycles).
const MAX_DEPTH: usize = 64;

/// Runtime is responsible for:
/// - mapping incoming event data to workflow variables
/// - interpolating strings using variables and globals
/// - dispatching actions to the low-level ActionExecutor
pub struct Runtime {
    config: Config,
    executor: ActionExecutor,
}

impl Runtime {
    /// Create a new runtime with the given config and dry-run mode.
    pub fn new(config: Config, dry_run: bool) -> Self {
        Self {
            config,
            executor: ActionExecutor::new(dry_run),
        }
    }

    /// Returns a reference to the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns a mutable reference to the configuration (e.g., to tweak globals at runtime).
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Enable or disable dry-run mode at runtime.
    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.executor.set_dry_run(dry_run);
    }

    /// Is dry-run currently enabled?
    pub fn is_dry_run(&self) -> bool {
        self.executor.is_dry_run()
    }

    /// Handle a raw event JSON object:
    /// - Expects a `"type"` field to select the appropriate workflow binding
    /// - Maps variables according to the event binding vars_map
    /// - Executes the referenced workflow
    pub fn run_event(&mut self, event: &Value) -> Result<()> {
        let event_type = event
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Event is missing string field 'type'"))?;

        let binding =
            self.config.events.get(event_type).cloned().ok_or_else(|| {
                anyhow::anyhow!("No event binding found for type '{}'", event_type)
            })?;

        self.run_binding(binding, event)
    }

    /// Run a known workflow by name with a provided variables map (skips the event->vars mapping).
    pub fn run_workflow_by_name(
        &mut self,
        workflow_name: &str,
        vars: HashMap<String, String>,
    ) -> Result<()> {
        self.execute_workflow(workflow_name, &Value::Null, vars)
    }

    /// Run an event binding (used by run_event)
    fn run_binding(&mut self, binding: EventBinding, event: &Value) -> Result<()> {
        let vars = self.vars_from_event(&binding, event)?;
        self.execute_workflow(&binding.workflow, event, vars)
    }

    /// Build the workflow-scoped variables map from an event according to the binding.
    fn vars_from_event(
        &self,
        binding: &EventBinding,
        event: &Value,
    ) -> Result<HashMap<String, String>> {
        let mut vars = HashMap::<String, String>::with_capacity(binding.vars_map.len());
        for (var_name, path) in &binding.vars_map {
            match get_json_path(event, path) {
                Some(v) => {
                    vars.insert(var_name.clone(), json_value_to_string(v));
                }
                None => {
                    warn!(
                        target: "notabot::runtime",
                        var = %var_name, path = %path,
                        "Event field not found for variable mapping; inserting empty string"
                    );
                    vars.insert(var_name.clone(), String::new());
                }
            }
        }
        Ok(vars)
    }

    /// Execute all steps in a named workflow with the provided variables.
    fn execute_workflow(
        &mut self,
        workflow_name: &str,
        event: &Value,
        mut vars: HashMap<String, String>,
    ) -> Result<()> {
        let steps = self
            .config
            .workflows
            .get(workflow_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown workflow '{}'", workflow_name))?;

        info!(
            target: "notabot::runtime",
            %workflow_name,
            steps = steps.len(),
            "Starting workflow"
        );

        for (idx, step) in steps.iter().enumerate() {
            trace!(
                target: "notabot::runtime",
                %workflow_name, step_index = idx,
                "Executing step"
            );
            self.execute_action(step, event, &mut vars, 0)
                .with_context(|| format!("Workflow '{}' failed at step {}", workflow_name, idx))?;
        }

        info!(
            target: "notabot::runtime",
            %workflow_name,
            "Workflow completed"
        );
        Ok(())
    }

    /// Execute a single action with recursion/sequence support.
    fn execute_action(
        &mut self,
        action: &ActionDef,
        event: &Value,
        vars: &mut HashMap<String, String>,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_DEPTH {
            bail!("Maximum action nesting depth ({MAX_DEPTH}) exceeded (possible cycle)");
        }

        match action {
            ActionDef::Sequence { steps } => {
                for (i, step) in steps.iter().enumerate() {
                    trace!(target: "notabot::runtime", depth, step_index = i, "Sequence step");
                    self.execute_action(step, event, vars, depth + 1)?;
                }
                Ok(())
            }

            ActionDef::Ref { name } => {
                let referenced = self
                    .config
                    .actions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Referenced action '{}' not found", name))?;
                trace!(target: "notabot::runtime", %name, depth, "Resolving Ref action");
                self.execute_action(&referenced, event, vars, depth + 1)
            }

            // Mouse
            ActionDef::MouseMove { x, y } => self.executor.mouse_move_to(*x, *y),
            ActionDef::MouseClick { button, count } => self.executor.mouse_click(*button, *count),
            ActionDef::MouseScroll { delta_x, delta_y } => {
                self.executor.mouse_scroll(*delta_x, *delta_y)
            }

            // Keyboard
            ActionDef::KeySeq { text } => {
                let s = self.interp(text, vars);
                self.executor.key_sequence(&s)
            }
            ActionDef::TypeText { text } => {
                let s = self.interp(text, vars);
                self.executor.type_text(&s)
            }

            // Timing
            ActionDef::SleepMs { ms } => self.executor.sleep_ms(*ms),
            ActionDef::SleepRandMs { min, max } => self.executor.sleep_rand_ms(*min, *max),

            // Window
            ActionDef::FocusWindow { title_contains } => {
                let title = self.interp(title_contains, vars);
                let focused = self.executor.focus_window(&title)?;
                if !focused {
                    warn!(
                        target: "notabot::runtime",
                        %title,
                        "focus_window reported no matching window"
                    );
                }
                Ok(())
            }

            // Logic & State
            ActionDef::SetVar { name, value } => {
                let k = self.interp(name, vars);
                let v = self.interp(value, vars);
                trace!(target: "notabot::runtime", key = %k, value = %v, "SetVar");
                vars.insert(k, v);
                Ok(())
            }
            ActionDef::Conditional {
                when,
                equals,
                then,
                else_,
            } => {
                let lhs = self.interp(when, vars);
                let rhs = self.interp(equals, vars);
                debug!(
                    target: "notabot::runtime",
                    when = %lhs, equals = %rhs, depth,
                    "Conditional evaluation"
                );
                if lhs == rhs {
                    self.execute_action(then, event, vars, depth + 1)
                } else if let Some(else_action) = else_ {
                    self.execute_action(else_action, event, vars, depth + 1)
                } else {
                    Ok(())
                }
            }

            // Logging
            ActionDef::Log { level, message } => {
                let msg = self.interp(message, vars);
                self.executor.log_message(*level, &msg);
                Ok(())
            }

            // Extensions (placeholders)
            ActionDef::OcrCheck {
                region,
                must_contain,
            } => {
                let text = self.interp(must_contain, vars);
                let _ok = self.executor.ocr_check(*region, &text)?;
                // In future, this could set a variable or influence control flow.
                Ok(())
            }
            ActionDef::CaptureScreen { path, region } => {
                let p = self.interp(path, vars);
                self.executor.capture_screen(&p, *region)
            }
        }
    }

    /// Interpolate a string with the current variables and config globals.
    fn interp(&self, s: &str, vars: &HashMap<String, String>) -> String {
        interpolation::interpolate_string(s, vars, &self.config.globals)
    }
}

/// Convert a JSON value to a user-friendly string:
/// - Strings are returned as-is.
/// - Numbers/bools are rendered via to_string().
/// - Arrays/objects are serialized as compact JSON.
fn json_value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Get a JSON value by a dotted path (e.g., "order.side").
fn get_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for seg in path.split('.') {
        match current {
            Value::Object(map) => {
                current = map.get(seg)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LogLevel;
    use serde_json::json;

    #[test]
    fn test_get_json_path() {
        let v = json!({"a":{"b":{"c":"ok"}}});
        assert_eq!(
            get_json_path(&v, "a.b.c"),
            Some(&Value::String("ok".into()))
        );
        assert!(get_json_path(&v, "a.b.x").is_none());
        assert!(get_json_path(&v, "a.b.c.d").is_none());
    }

    #[test]
    fn test_vars_from_event() {
        let mut cfg = Config::default();
        cfg.workflows.insert("wf".into(), vec![]);
        let binding = EventBinding {
            workflow: "wf".into(),
            vars_map: HashMap::from_iter([
                ("name".into(), "user.name".into()),
                ("age".into(), "user.age".into()),
                ("missing".into(), "not.there".into()),
            ]),
        };
        let event = json!({"type":"x","user":{"name":"Zied","age": 33}});

        let rt = Runtime::new(cfg, true);
        let vars = rt.vars_from_event(&binding, &event).unwrap();
        assert_eq!(vars.get("name").unwrap(), "Zied");
        assert_eq!(vars.get("age").unwrap(), "33");
        assert_eq!(vars.get("missing").unwrap(), "");
    }

    #[test]
    fn test_interp_with_globals() {
        let mut cfg = Config::default();
        cfg.globals
            .insert("app".into(), Value::String("Notabot".into()));
        let rt = Runtime::new(cfg, true);
        let mut vars = HashMap::new();
        vars.insert("user".into(), "Alice".into());

        let out = rt.interp("Hi {{user}} from {{@app}}", &vars);
        assert_eq!(out, "Hi Alice from Notabot");
    }

    #[test]
    fn test_workflow_runs_empty_sequence() {
        let mut cfg = Config::default();
        cfg.workflows.insert("empty".into(), vec![]);
        let mut rt = Runtime::new(cfg, true);
        rt.execute_workflow("empty", &Value::Null, HashMap::new())
            .unwrap();
    }

    #[test]
    fn test_conditional_equal_branch() {
        let mut cfg = Config::default();
        cfg.workflows.insert(
            "wf".into(),
            vec![ActionDef::Conditional {
                when: "{{x}}".into(),
                equals: "yes".into(),
                then: Box::new(ActionDef::Log {
                    level: LogLevel::Info,
                    message: "OK".into(),
                }),
                else_: None,
            }],
        );
        let mut rt = Runtime::new(cfg, true);
        let mut vars = HashMap::new();
        vars.insert("x".into(), "yes".into());
        rt.execute_workflow("wf", &Value::Null, vars).unwrap();
    }
}
