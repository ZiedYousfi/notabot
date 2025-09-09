use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::Value;
use tokio::{fs as afs, sync::mpsc::Sender, task::JoinHandle, time::interval};
use tracing::{error, info, trace, warn};

use super::EventSource;

/// Directory-based event source (polling).
///
/// Behavior:
/// - Recursively (optional) scans a directory at a fixed cadence (400ms default inside implementation)
///   adding new files that match an optional simple pattern (supports `*` wildcards).
/// - Processes at most one file per tick (FIFO) to smooth bursts:
///     1. Reads file contents (async).
///     2. Trims whitespace; skips empty files.
///     3. Parses as JSON; on success sends value through channel and deletes the file.
///        (Deletion prevents re-processing and acts as acknowledgement.)
///     4. On parse failure, logs a warning and leaves file for retry (allowing external fix).
///
/// Backpressure:
/// - Uses `sender.send(value).await` which awaits if the channel is full.
///
/// Error Handling:
/// - All I/O / parse errors are logged and the loop continues.
/// - Channel closure (send error) terminates the task gracefully.
///
/// Pattern Matching:
/// - Simple glob-like matching with `*` as "match any (possibly empty) substring".
///   Multiple `*` allowed. (E.g. `event_*.json`, `*order*`, `*.json`)
///
/// Future Enhancements:
/// - Optional `notify`-based watcher (platform dependent) instead of polling.
/// - Rate limits / metrics (processed, failed, skipped, retried).
/// - Configurable poll interval.
///
/// Safety:
/// - Never panics inside the task; designed for long-running robustness.
#[derive(Debug, Clone)]
pub struct DirectorySource {
    path: String,
    pattern: Option<String>,
    recursive: bool,
    poll_ms: u64,
}

impl DirectorySource {
    /// Create a new `DirectorySource`.
    ///
    /// `poll_ms` currently fixed internally to 400ms to keep signature stable;
    /// expose via config if needed later. For now we keep the parameter off the public
    /// API to avoid premature complexity—modify here if you want configurability.
    pub fn new(path: String, pattern: Option<String>, recursive: bool) -> Self {
        Self {
            path,
            pattern,
            recursive,
            poll_ms: 400,
        }
    }

    // fn make_interval(&self) -> Interval {
    //     interval(Duration::from_millis(self.poll_ms))
    // }
}

impl EventSource for DirectorySource {
    fn name(&self) -> &'static str {
        "directory"
    }

    fn start(&self, sender: Sender<Value>) -> JoinHandle<()> {
        let root = self.path.clone();
        let pattern = self.pattern.clone();
        let recursive = self.recursive;
        let poll_ms = self.poll_ms;

        tokio::spawn(async move {
            info!(
                target: "notabot::sources",
                path = %root, ?pattern, recursive, poll_ms,
                "DirectorySource task started (polling)"
            );

            let mut queue: VecDeque<PathBuf> = VecDeque::new();
            let mut queued: HashSet<PathBuf> = HashSet::new();
            let mut ticker = interval(Duration::from_millis(poll_ms));

            loop {
                ticker.tick().await;

                // Discover new candidate files
                discover_files(
                    Path::new(&root),
                    recursive,
                    &pattern,
                    &mut queue,
                    &mut queued,
                );

                // Process at most one file per tick for smoother throughput
                if let Some(path) = queue.pop_front() {
                    queued.remove(&path);

                    match afs::read_to_string(&path).await {
                        Ok(contents) => {
                            let trimmed = contents.trim();
                            if trimmed.is_empty() {
                                trace!(
                                    target: "notabot::sources",
                                    file = %path.display(),
                                    "Skipping empty file"
                                );
                                continue;
                            }
                            match serde_json::from_str::<Value>(trimmed) {
                                Ok(val) => {
                                    if let Err(e) = sender.send(val).await {
                                        error!(
                                            target: "notabot::sources",
                                            file = %path.display(),
                                            error = %e,
                                            "Channel closed; DirectorySource exiting"
                                        );
                                        break;
                                    }
                                    info!(
                                        target: "notabot::sources",
                                        file = %path.display(),
                                        "Dispatched event from directory file"
                                    );
                                    if let Err(e) = afs::remove_file(&path).await {
                                        warn!(
                                            target: "notabot::sources",
                                            file = %path.display(),
                                            error = %e,
                                            "Failed to delete file after dispatch"
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        target: "notabot::sources",
                                        file = %path.display(),
                                        error = %e,
                                        "Failed to parse JSON; leaving for retry"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                target: "notabot::sources",
                                file = %path.display(),
                                error = %e,
                                "Failed to read file"
                            );
                        }
                    }
                }
            }
        })
    }
}

/// Recursively (optional) discover files and enqueue new ones that match the pattern.
fn discover_files(
    root: &Path,
    recursive: bool,
    pattern: &Option<String>,
    queue: &mut VecDeque<PathBuf>,
    queued: &mut HashSet<PathBuf>,
) {
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();

        if path.is_dir() {
            if recursive {
                discover_files(&path, true, pattern, queue, queued);
            }
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(f) => f,
            None => continue,
        };

        if let Some(p) = pattern {
            if !simple_pattern_match(file_name, p) {
                continue;
            }
        }

        if queued.contains(&path) {
            continue;
        }

        queue.push_back(path.clone());
        queued.insert(path);
    }
}

/// Very small glob-like matcher supporting `*` wildcards (match any substring).
/// Multiple `*` supported. Case-sensitive.
fn simple_pattern_match(text: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return text == pattern;
    }

    let mut parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return true;
    }

    let starts_with_star = pattern.starts_with('*');
    let ends_with_star = pattern.ends_with('*');

    // Trim leading/trailing empties from boundary stars
    if starts_with_star {
        if let Some(first) = parts.first() {
            if first.is_empty() {
                parts.remove(0);
            }
        }
    }
    if ends_with_star {
        if let Some(last) = parts.last() {
            if last.is_empty() {
                parts.pop();
            }
        }
    }

    let mut remainder = text;

    // First segment (prefix) if no leading star
    if !starts_with_star {
        if let Some(first) = parts.first() {
            if !remainder.starts_with(first) {
                return false;
            }
            remainder = &remainder[first.len()..];
            parts.remove(0);
        }
    }

    // Intermediate segments
    while parts.len() > 1 {
        let seg = parts.remove(0);
        if let Some(pos) = remainder.find(seg) {
            remainder = &remainder[pos + seg.len()..];
        } else {
            return false;
        }
    }

    // Last segment
    if let Some(last) = parts.first() {
        if !ends_with_star {
            remainder.ends_with(last)
        } else {
            remainder.contains(last)
        }
    } else {
        // No segments -> pattern was all stars
        starts_with_star || ends_with_star || remainder.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pattern_match() {
        assert!(simple_pattern_match("event_123.json", "event_*.json"));
        assert!(simple_pattern_match("data.txt", "*.txt"));
        assert!(simple_pattern_match("abc", "abc"));
        assert!(!simple_pattern_match("abc", "abcd"));
        assert!(simple_pattern_match("abcd", "a*d"));
        assert!(simple_pattern_match("axyzd", "a*z*d"));
        assert!(!simple_pattern_match("abcd", "a*z*c"));
        assert!(simple_pattern_match("anything", "*"));
    }

    #[test]
    fn queue_dedup_logic_demo() {
        // This test only ensures helper functions compile & basic logic stands.
        // Full integration tests would require temp dirs and async runtime.
        let mut q = VecDeque::new();
        let mut s = HashSet::new();
        // simulate adding same file twice
        let f = PathBuf::from("file.json");
        q.push_back(f.clone());
        s.insert(f.clone());
        // second attempt should be skipped manually; we emulate discover_files logic:
        assert!(s.contains(&f));
    }
}
