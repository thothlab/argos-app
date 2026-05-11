//! WebSocket client.
//!
//! Wraps `tokio-tungstenite` with a connection-handle API tuned for the
//! Tauri shell: callers spawn a connection, push outgoing messages,
//! and receive incoming traffic on a channel. Each connection has its
//! own task; closing the handle drops the task and the socket.
//!
//! The handle itself is `Send + Sync` so it can live in Tauri app
//! state behind a `Mutex<HashMap<…>>`.

use std::time::SystemTime;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http, Message};

/// Outgoing command from the host shell to a live WS connection.
#[derive(Debug)]
pub enum WsCommand {
    /// Send a UTF-8 text frame with the given payload.
    SendText(String),
    /// Initiate a graceful close handshake.
    Close,
}

/// Direction tag attached to log entries — matches the UI's ↑ / ↓
/// rendering of the messages timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsDirection {
    /// Frame received from the peer.
    Incoming,
    /// Frame sent by the local client.
    Outgoing,
}

impl WsDirection {
    /// Stable string used by the Tauri serialisation layer.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

/// One event surfaced to the shell — either a state change or a
/// message that flowed in either direction.
#[derive(Debug, Clone)]
pub enum WsEvent {
    /// The handshake completed and the socket is open.
    Connected,
    /// A text frame either arrived or was successfully sent.
    Message {
        /// Which side the frame was on the wire from.
        direction: WsDirection,
        /// Decoded UTF-8 body of the frame.
        body: String,
        /// Unix ms timestamp at which the event reached the runtime.
        timestamp_ms: u128,
    },
    /// A non-text frame arrived. We don't render bytes in the UI yet
    /// — the event lets the host show "binary message (N bytes)".
    Binary {
        /// Direction tag.
        direction: WsDirection,
        /// Frame size in bytes.
        bytes: usize,
        /// Unix ms timestamp at which the event reached the runtime.
        timestamp_ms: u128,
    },
    /// The socket was closed (cleanly or otherwise).
    Closed {
        /// Close-code from the peer, when one was sent.
        code: Option<u16>,
        /// Optional reason string from the peer.
        reason: String,
    },
    /// Connection-level error — handshake failure, TLS error, sudden
    /// drop. The connection is dead by the time this fires.
    Error(String),
}

/// Live WebSocket connection. Drop the handle to terminate the task
/// (the socket is closed implicitly when the join handle is aborted).
pub struct WsHandle {
    /// Outbound command channel. Frontends push `SendText` / `Close`.
    pub commands: mpsc::Sender<WsCommand>,
    /// Inbound event stream. Drained by the host shell and forwarded
    /// to the renderer.
    pub events: mpsc::Receiver<WsEvent>,
    /// Background task handle — aborted on `Drop`.
    pub task: JoinHandle<()>,
}

impl WsHandle {
    /// Send a text message — non-blocking on the runtime side.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying channel is closed (i.e. the
    /// connection task already exited).
    pub fn send_text(&self, body: String) -> Result<(), &'static str> {
        self.commands
            .try_send(WsCommand::SendText(body))
            .map_err(|_| "connection closed")
    }

    /// Initiate a graceful close.
    pub fn close(&self) {
        let _ = self.commands.try_send(WsCommand::Close);
    }
}

impl Drop for WsHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Connection parameters. `headers` and `subprotocols` are baked into
/// the upgrade request before the TLS handshake.
#[derive(Debug, Clone)]
pub struct WsConnectOptions {
    /// `ws://` or `wss://` URL.
    pub url: String,
    /// `Sec-WebSocket-Protocol` advertisements.
    pub subprotocols: Vec<String>,
    /// Additional HTTP headers on the upgrade request.
    pub headers: Vec<(String, String)>,
}

/// Spawn a new WebSocket connection task.
///
/// The returned [`WsHandle`] exposes the command channel + event
/// stream. The task takes care of: handshake, reading frames, writing
/// frames from `commands`, emitting [`WsEvent`]s.
///
/// # Errors
///
/// Returns an error if the URL fails to parse into a request — TLS
/// and network errors surface as [`WsEvent::Error`] inside the task.
pub fn connect(opts: WsConnectOptions) -> Result<WsHandle, String> {
    let mut request = opts
        .url
        .clone()
        .into_client_request()
        .map_err(|e| format!("bad url: {e}"))?;
    let headers = request.headers_mut();
    for (k, v) in &opts.headers {
        if let (Ok(name), Ok(value)) =
            (http::HeaderName::try_from(k.as_str()), http::HeaderValue::try_from(v.as_str()))
        {
            headers.append(name, value);
        }
    }
    if !opts.subprotocols.is_empty() {
        let joined = opts.subprotocols.join(", ");
        if let Ok(v) = http::HeaderValue::try_from(joined.as_str()) {
            headers.insert(http::header::SEC_WEBSOCKET_PROTOCOL, v);
        }
    }

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<WsCommand>(16);
    let (evt_tx, evt_rx) = mpsc::channel::<WsEvent>(64);

    let task = tokio::spawn(async move {
        // tokio-tungstenite's connect_async returns the upgraded stream
        // plus the HTTP response. We don't surface response headers to
        // the host in v1.
        let stream = match tokio_tungstenite::connect_async(request).await {
            Ok((s, _resp)) => s,
            Err(e) => {
                let _ = evt_tx.send(WsEvent::Error(format!("connect: {e}"))).await;
                return;
            }
        };
        let _ = evt_tx.send(WsEvent::Connected).await;

        let (mut sink, mut stream) = stream.split();

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(WsCommand::SendText(body)) => {
                        match sink.send(Message::Text(body.clone())).await {
                            Ok(()) => {
                                let _ = evt_tx
                                    .send(WsEvent::Message {
                                        direction: WsDirection::Outgoing,
                                        body,
                                        timestamp_ms: now_ms(),
                                    })
                                    .await;
                            }
                            Err(e) => {
                                let _ = evt_tx.send(WsEvent::Error(format!("send: {e}"))).await;
                                return;
                            }
                        }
                    }
                    Some(WsCommand::Close) | None => {
                        let _ = sink.send(Message::Close(None)).await;
                        let _ = evt_tx
                            .send(WsEvent::Closed { code: None, reason: "client closed".into() })
                            .await;
                        return;
                    }
                },
                frame = stream.next() => match frame {
                    Some(Ok(Message::Text(t))) => {
                        let _ = evt_tx
                            .send(WsEvent::Message {
                                direction: WsDirection::Incoming,
                                body: t,
                                timestamp_ms: now_ms(),
                            })
                            .await;
                    }
                    Some(Ok(Message::Binary(b))) => {
                        let _ = evt_tx
                            .send(WsEvent::Binary {
                                direction: WsDirection::Incoming,
                                bytes: b.len(),
                                timestamp_ms: now_ms(),
                            })
                            .await;
                    }
                    Some(Ok(Message::Ping(p))) => {
                        // Auto-reply to keep-alives so the connection
                        // doesn't time out behind a proxy.
                        let _ = sink.send(Message::Pong(p)).await;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let (code, reason) = frame
                            .map_or((None, String::new()), |f| (Some(u16::from(f.code)), f.reason.to_string()));
                        let _ = evt_tx.send(WsEvent::Closed { code, reason }).await;
                        return;
                    }
                    Some(Ok(Message::Pong(_) | Message::Frame(_))) => { /* ignore */ }
                    Some(Err(e)) => {
                        let _ = evt_tx.send(WsEvent::Error(format!("recv: {e}"))).await;
                        return;
                    }
                    None => {
                        let _ = evt_tx
                            .send(WsEvent::Closed { code: None, reason: "stream ended".into() })
                            .await;
                        return;
                    }
                }
            }
        }
    });

    Ok(WsHandle {
        commands: cmd_tx,
        events: evt_rx,
        task,
    })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt as _;
    use futures_util::StreamExt as _;
    use tokio::net::TcpListener;

    /// Spin up a one-shot WebSocket server that echoes every text
    /// frame it receives. Returns the bound `ws://` URL.
    async fn echo_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                while let Some(Ok(msg)) = ws.next().await {
                    match msg {
                        Message::Text(t) => {
                            let _ = ws.send(Message::Text(format!("echo: {t}"))).await;
                        }
                        Message::Close(_) => return,
                        _ => {}
                    }
                }
            }
        });
        format!("ws://{addr}/")
    }

    #[tokio::test]
    async fn connects_and_echoes_text_message() {
        let url = echo_server().await;
        let mut handle = connect(WsConnectOptions {
            url,
            subprotocols: Vec::new(),
            headers: Vec::new(),
        })
        .unwrap();

        // Connected event first.
        let first = tokio::time::timeout(std::time::Duration::from_secs(2), handle.events.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first, WsEvent::Connected));

        handle.send_text("hi".into()).unwrap();

        // Drain events until we see both an outgoing "hi" and an
        // incoming "echo: hi". Ordering between outgoing-echo and the
        // server reply isn't guaranteed.
        let mut saw_outgoing = false;
        let mut saw_incoming = false;
        for _ in 0..6 {
            let ev = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                handle.events.recv(),
            )
            .await
            .unwrap()
            .unwrap();
            if let WsEvent::Message {
                direction,
                body,
                ..
            } = &ev
            {
                if *direction == WsDirection::Outgoing && body == "hi" {
                    saw_outgoing = true;
                }
                if *direction == WsDirection::Incoming && body == "echo: hi" {
                    saw_incoming = true;
                }
            }
            if saw_outgoing && saw_incoming {
                break;
            }
        }
        assert!(saw_outgoing && saw_incoming);
        handle.close();
    }

    #[tokio::test]
    async fn invalid_url_returns_immediate_error() {
        let res = connect(WsConnectOptions {
            url: "::not-a-url::".into(),
            subprotocols: Vec::new(),
            headers: Vec::new(),
        });
        let err = match res {
            Ok(_) => panic!("expected error for malformed URL"),
            Err(e) => e,
        };
        assert!(err.contains("bad url"));
    }
}
