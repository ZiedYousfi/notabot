# Notabot

A modular, extensible wrapper around the [Enigo](https://crates.io/crates/enigo) library for declarative UI automation. Define events, actions, and workflows in JSON configuration files, and let the runtime handle execution. Perfect for automating repetitive tasks, testing UIs, or creating custom macros while keeping your code clean and configurable.

Built with Rust for reliability, performance, and safety. Supports dry-run mode for safe testing without actual input simulation.

## Features

- **Declarative Configuration**: Define automation flows entirely in JSON—no Rust code changes needed.
- **Multiple Event Sources**: Pull events from files, directories, TCP sockets, or stdin.
- **Rich Action Set**: Mouse movements, clicks, keyboard sequences, sleeps, window focusing, logging, conditionals, and extensible for OCR/screen capture.
- **Variable Interpolation**: Embed dynamic values from events or globals (e.g., `{{symbol}}` for symbols, `{{@app_name}}` for globals).
- **Modular Architecture**: Separate concerns with crates for config, executor, sources, and utils.
- **Dry-Run Mode**: Simulate actions without performing them—great for debugging.
- **Logging & Tracing**: Built-in structured logging with levels (trace, debug, info, warn, error).
- **Async Support**: Non-blocking event processing with Tokio for high-throughput scenarios.
- **Validation**: JSON schema support for config validation.
- **Cross-Platform**: Primarily Windows-focused (via `windows` crate for window management), but Enigo handles macOS/Linux input simulation.

## Quick Start

### Prerequisites

- Rust 1.88+ (stable channel)
- Cargo

### Installation

1. Clone or create the project:
   ```bash
   git clone https://github.com/ZiedYousfi/notabot notabot
   cd notabot
   ```

2. Build and run:
   ```bash
   cargo build --release
   cargo run -- --config config/default.json --dry-run
   ```

   - `--dry-run`: Test without simulating input.
   - `--log-level debug`: Increase verbosity.

### Basic Usage

1. **Configure**: Edit `config/default.json` to define your sources, actions, workflows, and events.
2. **Trigger Events**: Drop a JSON event file into the configured path (e.g., `C:/Users/Public/enigo_event.json`).
3. **Execute**: Run the binary. It watches for events and executes the matching workflow.

Example event JSON:
```json
{
  "type": "send_text",
  "text": "Hello, Zied! 🌸"
}
```

This triggers the `send_text` event, which runs the `send_message` workflow (focusing a window, typing text, and pressing Enter).

## Configuration

All automation is driven by a JSON config file. See `config/schema.json` for the full schema.

### Key Sections

- **sources**: Array of event input methods.
  - `file`: Watch a single file path.
  - `directory`: Watch a folder for new files (FIFO processing).
  - `tcp`: Listen on a TCP address for JSON events.
  - `stdin`: Read from standard input (for piping).

- **actions**: Reusable building blocks (named for reference).
  - Examples: `mouse_move`, `key_seq` with interpolation like `{{message}}`.

- **workflows**: Named sequences of actions (can reference other actions or inline sequences).

- **events**: Map event types to workflows + variable mappings.
  - e.g., `"send_text": { "workflow": "send_message", "vars_map": { "message": "text" } }`

- **globals**: Key-value pairs for cross-workflow variables (accessed as `{{@global_key}}`).

Validation is automatic on load. Use tools like `jsonschema` to validate against `schema.json`.

### Example Config Snippet

See `config/default.json` for a full example that focuses Notepad, types a message, and logs it.

## Event Sources

Events are JSON objects with a `type` and arbitrary `data` fields. The runtime processes them asynchronously.

- **File Source**: Polls a file every 100ms; processes and deletes on success.
- **Directory Source**: Uses `notify` crate for filesystem events; filters by pattern (e.g., `event_*`).
- **TCP Source**: Listens for connections; parses JSON from streams and sends ACK ("OK" or "ERROR").

Extend by implementing the `EventSource` trait.

## Actions

Supported actions include:

- **Input Simulation** (via Enigo):
  - `mouse_move { x: 960, y: 540 }`
  - `mouse_click { button: "left" }`
  - `key_seq { text: "{WIN}rnotepad{ENTER}" }` (supports Enigo's key syntax)
  - `type_text { text: "{{dynamic_value}}" }`

- **Timing & Control**:
  - `sleep_ms { ms: 500 }`
  - `sleep_rand_ms { min: 100, max: 300 }` (adds human-like variability)

- **Window Management**:
  - `focus_window { title_contains: "Calculator" }` (uses Win32 API)

- **Logic & State**:
  - `set_var { name: "counter", value: "1" }`
  - `conditional { when: "{{side}}", equals: "buy", then: ..., else: ... }`

- **Logging**:
  - `log { level: "info", message: "Event processed: {{type}}" }`

- **Extensions** (placeholders for future impl):
  - `ocr_check { region: [0, 0, 1920, 1080], must_contain: "Success" }`
  - `capture_screen { path: "screenshot.png", region: [100, 100, 200, 200] }`

Actions support recursion (sequences, references) and interpolation for dynamism.

## Workflows & Events

Workflows are arrays of `ActionDef` (single actions, sequences, or refs). Events bind incoming JSON types to workflows, mapping fields to variables.

Example: An event with `"type": "calculate_sum"` maps `"first_number"` to `{{num1}}` in the workflow.

## Examples

- **Simple Macro** (`examples/simple_macro.json`): Opens Notepad, types "Hello, World!", and saves it.
  - Trigger: `{ "type": "run_hello" }`

- **Multi-Step Workflow** (`examples/multi_step_workflow.json`): Focuses Calculator, adds two numbers, and logs the operation.
  - Trigger: `{ "type": "calculate_sum", "first_number": "5", "second_number": "3" }`

Run with:
```bash
cargo run -- --config examples/simple_macro.json
```

## Development

### Building

```bash
cargo build
cargo test
cargo clippy  # Linting
```

### Adding New Actions

1. Add to `src/config/models.rs` enum `Action`.
2. Update `src/config/schema.json` with validation rules.
3. Implement in `src/executor/runtime.rs` `execute_action` match arm.
4. Add tests in `tests/integration_test.rs`.

### Testing

- Unit: `cargo test` (covers interpolation, parsing).
- Integration: Simulate events and assert logs/actions (uses `--dry-run`).
- Example: `tests/integration_test.rs` validates config loading and basic execution.

### Extending Sources

Implement `EventSource` trait in `src/sources/` and add to `manager.rs`.

### Troubleshooting

- **Window Focus Fails**: Ensure Windows API permissions; test on target machine.
- **Interpolation Issues**: Use `{{@global}}` for globals, `{{var}}` for event vars.
- **Event Not Processed**: Check logs for parsing errors; validate JSON against schema.
- **Performance**: For high-volume, use TCP source; adjust poll intervals.

## Contributing

Contributions welcome! Fork, create a feature branch, add tests, and submit a PR.

1. Fork the repo.
2. Create your feature branch (`git checkout -b feature/AmazingNewFeature`).
3. Commit changes (`git commit -m 'Add some AmazingNewFeature'`).
4. Push to the branch (`git push origin feature/AmazingNewFeature`).
5. Open a Pull Request.

Please include tests and update the schema/docs as needed.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. (Add a LICENSE file if not present.)

## Acknowledgments

- [Enigo](https://github.com/enigo-rs/enigo): Core input simulation.
- [Serde](https://serde.rs/): JSON handling.
- [Tokio](https://tokio.rs/): Async runtime.

---

*Version 0.1.0* | *Built with ❤️ in Rust*