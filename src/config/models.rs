use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Root configuration for Notabot.
///
/// This structure is intended to be deserialized from a JSON configuration file.
/// It captures all the building blocks the runtime needs:
/// - event `sources`
/// - reusable/named `actions`
/// - `workflows` (named sequences of actions)
/// - `events` bindings (event type -> workflow + variable mapping)
/// - global variables available across workflows (`globals`)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Config {
    /// Event input sources (file, directory, tcp, stdin).
    #[serde(default)]
    pub sources: Vec<SourceConfig>,

    /// Reusable named actions (macros, composites, or single actions).
    /// You can reference one by using: `{ "type": "ref", "name": "my_action" }`.
    #[serde(default)]
    pub actions: NamedActions,

    /// Named workflows, each a sequence of action definitions.
    /// Events typically refer to a workflow by name.
    #[serde(default)]
    pub workflows: Workflows,

    /// Event bindings mapping an incoming event's `type` to a workflow and variable mapping.
    #[serde(default)]
    pub events: EventMap,

    /// Global variables accessible via interpolation (e.g., `{{@app_name}}`).
    /// Values can be any JSON value (string/number/bool/object/array).
    #[serde(default)]
    pub globals: GlobalsMap,
}

/// A convenient alias for named action map.
pub type NamedActions = BTreeMap<String, ActionDef>;

/// Workflows are named lists of `ActionDef`s.
pub type Workflows = BTreeMap<String, Vec<ActionDef>>;

/// Map of incoming event type -> binding.
pub type EventMap = BTreeMap<String, EventBinding>;

/// Global variables.
pub type GlobalsMap = BTreeMap<String, serde_json::Value>;

/// Variable mapping: workflow variable -> event field path/key
/// Example:
///   { "vars_map": { "message": "text", "side": "order.side" } }
pub type VarsMap = HashMap<String, String>;

/// Event binding definition: connects an incoming event `type` to a workflow and
/// optionally maps JSON fields from the event into workflow variables.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventBinding {
    /// Workflow name to execute for this event type.
    pub workflow: String,

    /// Map workflow variables to event fields.
    /// Example: `{ "message": "text" }` maps `{{message}}` to event's `text` field.
    #[serde(default)]
    pub vars_map: VarsMap,
}

/// Event source configuration.
/// Use `type` to select a variant:
/// - "file": watch/read a single file repeatedly
/// - "directory": watch a directory for new files
/// - "tcp": listen on a TCP socket for JSON messages
/// - "stdin": read newline-delimited JSON from standard input
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceConfig {
    /// Poll a single file for JSON events.
    File {
        /// Absolute or relative path to the file.
        path: String,
        /// Poll interval in milliseconds (default: 100).
        #[serde(default)]
        poll_ms: Option<u64>,
        /// Delete the file after a successful read/parse (default: false).
        #[serde(default)]
        delete_on_success: Option<bool>,
    },

    /// Watch a directory for new files that contain JSON events.
    Directory {
        /// Directory to watch.
        path: String,
        /// Optional file name pattern (e.g., "event_*" or "*.json").
        #[serde(default)]
        pattern: Option<String>,
        /// Whether to watch subdirectories (default: false).
        #[serde(default)]
        recursive: Option<bool>,
    },

    /// Listen on a TCP address (e.g., "127.0.0.1:5000") for JSON events.
    Tcp {
        /// Bind address and port.
        bind: String,
        /// Whether to send an ACK ("OK"/"ERROR") after processing (default: true).
        #[serde(default)]
        ack: Option<bool>,
    },

    /// Read JSON events from standard input (newline-delimited).
    Stdin,
}

/// Action definition.
///
/// This is the heart of the runtime. Actions can be:
/// - primitives (mouse, keyboard, timing, logging, etc.)
/// - composites (`sequence`)
/// - references to named actions (`ref`)
///
/// By default, all string fields support interpolation with:
/// - workflow variables: `{{var_name}}`
/// - globals: `{{@global_key}}`
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionDef {
    /// A sequence of actions executed in order.
    Sequence { steps: Vec<ActionDef> },

    /// Reference a named action from the `actions` map.
    Ref {
        /// The name of the action to reference.
        name: String,
    },

    // --- Input: Mouse ---
    /// Move the mouse cursor to an absolute screen position.
    MouseMove { x: i32, y: i32 },

    /// Click a mouse button one or more times.
    MouseClick {
        button: MouseButton,
        /// Number of clicks (default: 1).
        #[serde(default)]
        count: Option<u8>,
    },

    /// Scroll the mouse wheel (pixels/lines; interpretation depends on executor).
    /// Positive values typically indicate scrolling down/right; negative up/left.
    MouseScroll {
        /// Horizontal scroll delta.
        #[serde(default)]
        delta_x: i32,
        /// Vertical scroll delta.
        #[serde(default)]
        delta_y: i32,
    },

    // --- Input: Keyboard ---
    /// Send a raw key sequence using Enigo's syntax
    /// e.g., "{WIN}rnotepad{ENTER}"
    KeySeq { text: String },

    /// Type literal text (handles unicode).
    TypeText { text: String },

    // --- Timing & Control ---
    /// Sleep for a fixed duration in milliseconds.
    SleepMs { ms: u64 },

    /// Sleep for a random duration in milliseconds within [min, max].
    SleepRandMs { min: u64, max: u64 },

    // --- Window Management ---
    /// Attempt to focus a window whose title contains the given substring.
    FocusWindow { title_contains: String },

    // --- Logic & State ---
    /// Set (or override) a workflow-scoped variable.
    SetVar { name: String, value: String },

    /// Conditionally execute `then` or `else` based on string equality:
    /// if interpolate(when) == interpolate(equals) => then, else otherwise.
    Conditional {
        /// Left-hand side string (interpolated).
        when: String,
        /// Right-hand side string (interpolated).
        equals: String,
        /// Action to run if the condition holds.
        then: Box<ActionDef>,
        /// Optional action to run otherwise.
        #[serde(rename = "else")]
        #[serde(default)]
        else_: Option<Box<ActionDef>>,
    },

    // --- Logging ---
    /// Log a message with a chosen level.
    Log { level: LogLevel, message: String },

    // --- Extensions (placeholders) ---
    /// Check for OCR text presence in a region.
    OcrCheck {
        /// Screen region to scan. If omitted, implementation may use full screen.
        #[serde(default)]
        region: Option<Rect>,
        /// The text that must appear.
        must_contain: String,
    },

    /// Capture a screenshot to a file.
    CaptureScreen {
        /// Output file path (e.g., "screenshot.png").
        path: String,
        /// Optional region to capture.
        #[serde(default)]
        region: Option<Rect>,
    },
}

/// A rectangle region on screen.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Mouse button enumeration.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// Logging level enumeration.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
