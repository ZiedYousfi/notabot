use std::net::SocketAddr;

use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc::Sender,
    task::JoinHandle,
};
use tracing::{error, info, trace, warn};

use super::EventSource;

/// TCP-based event source.
///
/// Protocol:
/// - Each inbound connection may send one or more newline-delimited JSON values (NDJSON style).
/// - Each non-empty line is trimmed and parsed as JSON (must be a single JSON value, typically an object).
/// - If `ack = true`, the server replies with `OK\n` on success, or `ERROR <message>\n` on parse failure.
///
/// Behavior & Robustness:
/// - Connections are handled concurrently (one task per connection).
/// - Malformed JSON lines are logged with `warn!` and (optionally) receive an error ACK; the connection stays open.
/// - Channel backpressure is respected (`sender.send(...).await`).
/// - If the event channel is closed, the per-connection task terminates early.
/// - The listener task runs indefinitely until the process is shut down (no explicit stop signal implemented).
///
/// Security / Hardening Notes:
/// - No authentication is performed (intended for local / trusted network use).
/// - For production / untrusted networks, consider:
///     * TLS / mTLS
///     * Length limiting
///     * Rate limiting
///     * JSON schema validation at the source boundary
///
/// Future Enhancements:
/// - Support for batched JSON arrays splitting into multiple events.
/// - Optional framing (length-prefix) for binary-safe transport.
/// - Metrics (accepted connections, messages processed, errors).
#[derive(Debug, Clone)]
pub struct TcpSource {
    bind: String,
    ack: bool,
}

impl TcpSource {
    /// Create a new `TcpSource`.
    ///
    /// `bind` is the socket address to listen on (e.g. "127.0.0.1:5000").
    /// `ack` controls whether "OK"/"ERROR ..." lines are written back to clients.
    pub fn new(bind: String, ack: bool) -> Self {
        Self { bind, ack }
    }

    /// Spawn a task to handle a single accepted client connection.
    async fn handle_client(mut stream: TcpStream, sender: Sender<Value>, ack: bool) {
        let peer: SocketAddr = match stream.peer_addr() {
            Ok(a) => a,
            Err(e) => {
                warn!(target: "notabot::sources", error = %e, "Could not get peer address");
                return;
            }
        };

        let (read_half, mut write_half) = stream.split();
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();

        trace!(target: "notabot::sources", peer = %peer, "TCP client handler started");

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    trace!(target: "notabot::sources", peer = %peer, "TCP client closed connection");
                    break;
                }
                Ok(_) => {
                    let raw = line.trim();
                    if raw.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<Value>(raw) {
                        Ok(val) => {
                            if let Err(e) = sender.send(val).await {
                                error!(
                                    target: "notabot::sources",
                                    peer = %peer,
                                    error = %e,
                                    "Channel closed while sending TCP event; ending handler"
                                );
                                break;
                            }
                            if ack {
                                if let Err(e) = write_half.write_all(b"OK\n").await {
                                    warn!(
                                        target: "notabot::sources",
                                        peer = %peer,
                                        error = %e,
                                        "Failed to write OK ACK; closing connection"
                                    );
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                target: "notabot::sources",
                                peer = %peer,
                                error = %e,
                                line = raw,
                                "Invalid JSON from TCP client"
                            );
                            if ack {
                                // Best-effort error response; ignore failure.
                                let _ = write_half
                                    .write_all(format!("ERROR {e}\n").as_bytes())
                                    .await;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        target: "notabot::sources",
                        peer = %peer,
                        error = %e,
                        "Error reading from TCP client"
                    );
                    break;
                }
            }
        }

        trace!(target: "notabot::sources", peer = %peer, "TCP client handler ended");
    }
}

impl EventSource for TcpSource {
    fn name(&self) -> &'static str {
        "tcp"
    }

    fn start(&self, sender: Sender<Value>) -> JoinHandle<()> {
        let bind = self.bind.clone();
        let ack = self.ack;
        tokio::spawn(async move {
            info!(
                target: "notabot::sources",
                %bind, ack,
                "TcpSource listener starting"
            );

            let listener = match TcpListener::bind(&bind).await {
                Ok(l) => l,
                Err(e) => {
                    error!(
                        target: "notabot::sources",
                        %bind,
                        error = %e,
                        "Failed to bind TCP listener (terminating task)"
                    );
                    return;
                }
            };

            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        trace!(
                            target: "notabot::sources",
                            %bind,
                            client = %addr,
                            "Accepted TCP connection"
                        );
                        let s = sender.clone();
                        tokio::spawn(Self::handle_client(stream, s, ack));
                    }
                    Err(e) => {
                        warn!(
                            target: "notabot::sources",
                            %bind,
                            error = %e,
                            "Accept failed; continuing"
                        );
                        // Slight yield to avoid hot loop if persistent failure.
                        tokio::task::yield_now().await;
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constructor() {
        let src = TcpSource::new("127.0.0.1:5000".into(), true);
        assert_eq!(src.name(), "tcp");
        // binding correctness isn't validated here to keep test hermetic
    }

    #[tokio::test]
    async fn test_rejects_invalid_json_line() {
        // We test the helper parsing path indirectly by spinning up a listener on an ephemeral port.
        // This ensures `handle_client` path executes without fully asserting channel receipt.
        use tokio::io::AsyncWriteExt;
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<Value>(4);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ack = true;

        // Spawn accept loop for a single test connection then break.
        let accept_task = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                TcpSource::handle_client(stream, tx, ack).await;
            }
        });

        // Client sends invalid then valid JSON
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"{invalid json}\n").await.unwrap();
        client.write_all(b"{\"type\":\"x\"}\n").await.unwrap();
        // Give server a moment
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // Close client
        drop(client);

        // We should receive exactly one valid value
        let val = rx.recv().await.expect("expected one JSON event");
        assert_eq!(val.get("type").and_then(|v| v.as_str()), Some("x"));

        // Ensure task completes
        accept_task.await.unwrap();
    }
}
