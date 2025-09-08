use anyhow::{Context, Result};
use enigo::Keyboard as _;
use enigo::Mouse as _;
use enigo::{Axis, Button as EButton, Coordinate, Direction, Enigo, Settings};
use rand::random_range;
use std::thread;
use std::time::Duration;
use tracing::{debug, info, trace, warn};

use crate::config::models::{LogLevel, MouseButton as CMouseButton, Rect};
use crate::utils::window;

/// Executes low-level actions (mouse/keyboard/sleep/log) with optional dry-run mode.
/// In dry-run mode, actions are only logged and no real input is simulated.
pub struct ActionExecutor {
    dry_run: bool,
    enigo: Option<Enigo>,
}

impl ActionExecutor {
    /// Create a new executor.
    /// - dry_run: when true, only logs instead of simulating real input.
    pub fn new(dry_run: bool) -> Self {
        Self {
            dry_run,
            enigo: None,
        }
    }

    /// Returns whether the executor is currently in dry-run mode.
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Enable or disable dry-run mode dynamically.
    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run;
    }

    /// Move mouse cursor to absolute screen coordinates.
    pub fn mouse_move_to(&mut self, x: i32, y: i32) -> Result<()> {
        if self.dry_run {
            info!(target: "notabot::actions", x, y, "DRY-RUN mouse_move_to");
            return Ok(());
        }
        let enigo = self.ensure_enigo()?;
        trace!(target: "notabot::actions", x, y, "mouse_move_to");
        enigo.move_mouse(x, y, Coordinate::Abs)?;
        Ok(())
    }

    /// Click a mouse button one or more times.
    pub fn mouse_click(&mut self, button: CMouseButton, count: Option<u8>) -> Result<()> {
        let count = count.unwrap_or(1).max(1);
        if self.dry_run {
            info!(target: "notabot::actions", ?button, count, "DRY-RUN mouse_click");
            return Ok(());
        }
        let enigo = self.ensure_enigo()?;
        let btn = map_mouse_button(button);
        trace!(target: "notabot::actions", ?button, count, "mouse_click");
        for _ in 0..count {
            enigo.button(btn, Direction::Click)?;
        }
        Ok(())
    }

    /// Scroll the mouse wheel. Currently a best-effort implementation:
    /// If unsupported by the underlying enigo version, this will log a warning.
    pub fn mouse_scroll(&mut self, delta_x: i32, delta_y: i32) -> Result<()> {
        if self.dry_run {
            info!(target: "notabot::actions", delta_x, delta_y, "DRY-RUN mouse_scroll");
            return Ok(());
        }
        let enigo = self.ensure_enigo()?;
        trace!(target: "notabot::actions", delta_x, delta_y, "mouse_scroll");
        if delta_x != 0 {
            let _ = enigo.scroll(delta_x, Axis::Horizontal);
        }
        if delta_y != 0 {
            let _ = enigo.scroll(delta_y, Axis::Vertical);
        }
        Ok(())
    }

    /// Send a key sequence. Supports enigo's special key syntax like "{ENTER}".
    pub fn key_sequence(&mut self, text: &str) -> Result<()> {
        if self.dry_run {
            info!(target: "notabot::actions", %text, "DRY-RUN key_sequence");
            return Ok(());
        }
        let enigo = self.ensure_enigo()?;
        trace!(target: "notabot::actions", %text, "key_sequence");
        let _ = enigo.text(text);
        Ok(())
    }

    /// Type literal text (unicode).
    /// Implementation uses enigo's `key_sequence`, which handles plain text well.
    pub fn type_text(&mut self, text: &str) -> Result<()> {
        if self.dry_run {
            info!(target: "notabot::actions", %text, "DRY-RUN type_text");
            return Ok(());
        }
        let enigo = self.ensure_enigo()?;
        trace!(target: "notabot::actions", %text, "type_text");
        let _ = enigo.text(text);
        Ok(())
    }

    /// Sleep for a fixed duration in milliseconds (blocking).
    /// Consider using an async sleep in higher-level async contexts.
    pub fn sleep_ms(&self, ms: u64) -> Result<()> {
        if self.dry_run {
            info!(target: "notabot::actions", ms, "DRY-RUN sleep_ms");
            return Ok(());
        }
        trace!(target: "notabot::actions", ms, "sleep_ms");
        thread::sleep(Duration::from_millis(ms));
        Ok(())
    }

    /// Sleep for a random duration in milliseconds within [min, max] inclusive (blocking).
    pub fn sleep_rand_ms(&self, min: u64, max: u64) -> Result<()> {
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        let delay = if lo == hi { lo } else { random_range(lo..=hi) };
        if self.dry_run {
            info!(target: "notabot::actions", min = lo, max = hi, delay, "DRY-RUN sleep_rand_ms");
            return Ok(());
        }
        trace!(target: "notabot::actions", min = lo, max = hi, delay, "sleep_rand_ms");
        thread::sleep(Duration::from_millis(delay));
        Ok(())
    }

    /// Try to focus a window with title containing the substring.
    /// Returns Ok(true) if a window was focused.
    pub fn focus_window(&self, title_contains: &str) -> Result<bool> {
        if self.dry_run {
            info!(target: "notabot::actions", %title_contains, "DRY-RUN focus_window");
            return Ok(false);
        }
        trace!(target: "notabot::actions", %title_contains, "focus_window");
        let focused = window::focus_window(title_contains)
            .with_context(|| format!("focus_window({title_contains}) failed"))?;
        if focused {
            debug!(target: "notabot::actions", %title_contains, "focus_window: focused=true");
        } else {
            warn!(target: "notabot::actions", %title_contains, "focus_window: no match");
        }
        Ok(focused)
    }

    /// Log a message with a given level, useful within workflows.
    pub fn log_message(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Trace => trace!(target: "notabot", "{message}"),
            LogLevel::Debug => debug!(target: "notabot", "{message}"),
            LogLevel::Info => info!(target: "notabot", "{message}"),
            LogLevel::Warn => warn!(target: "notabot", "{message}"),
            LogLevel::Error => tracing::error!(target: "notabot", "{message}"),
        }
    }

    /// Placeholder for OCR check. Not implemented; logs intent and returns Ok(false).
    pub fn ocr_check(&self, region: Option<Rect>, must_contain: &str) -> Result<bool> {
        if self.dry_run {
            info!(target: "notabot::actions", ?region, %must_contain, "DRY-RUN ocr_check");
            return Ok(true);
        }
        warn!(
            target: "notabot::actions",
            ?region, %must_contain,
            "ocr_check not implemented; returning false"
        );
        Ok(false)
    }

    /// Placeholder for screen capture. Not implemented; logs intent and returns Ok(()).
    pub fn capture_screen(&self, path: &str, region: Option<Rect>) -> Result<()> {
        if self.dry_run {
            info!(target: "notabot::actions", %path, ?region, "DRY-RUN capture_screen");
            return Ok(());
        }
        warn!(
            target: "notabot::actions",
            %path, ?region,
            "capture_screen not implemented"
        );
        Ok(())
    }

    fn ensure_enigo(&mut self) -> Result<&mut Enigo> {
        if self.enigo.is_none() {
            trace!(target: "notabot::actions", "Initializing Enigo");
            self.enigo =
                Some(Enigo::new(&Settings::default()).context("Failed to initialize Enigo")?);
        }
        Ok(self.enigo.as_mut().expect("Enigo must be initialized"))
    }
}

fn map_mouse_button(btn: CMouseButton) -> EButton {
    match btn {
        CMouseButton::Left => EButton::Left,
        CMouseButton::Middle => EButton::Middle,
        CMouseButton::Right => EButton::Right,
    }
}
