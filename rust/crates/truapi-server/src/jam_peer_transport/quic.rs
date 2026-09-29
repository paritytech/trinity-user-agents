//! Host-terminated JAMNP-S QUIC connections with per-execution caps.
//!
//! One [`Transport`] is one execution's peer-transport authority: it owns a
//! QUIC endpoint on an ephemeral UDP port, the local identity and every
//! connection and stream the guest holds. Only `dial` and `open` wait, for the
//! QUIC handshake or the peer's stream credit, and both are bounded. The work
//! runs on the transport's own tokio runtime, so any executor may await it.
//! The guest never sees a length prefix: the host frames outgoing messages and
//! reassembles incoming ones.

use std::collections::{HashMap, VecDeque};
use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use truapi::latest;

use super::peer_id;
use super::tls::{self, Identity, IdentityError};

/// Connections one execution may hold.
pub(super) const MAX_CONNECTIONS: usize = latest::JAM_PEER_TRANSPORT_MAX_CONNECTIONS as usize;
/// Streams one execution may hold per connection.
const MAX_STREAMS_PER_CONNECTION: usize =
    latest::JAM_PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION as usize;
/// Largest message in either direction, without its length prefix.
const MAX_MESSAGE_BYTES: usize = latest::JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES as usize;
/// Bytes buffered per connection (outgoing not yet written plus incoming not
/// yet received by the guest) before sends fail and reads pause.
const MAX_BUFFERED_BYTES_PER_CONNECTION: usize =
    latest::JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION as usize;
/// Undrained events kept for the guest; later ones are dropped.
const MAX_PENDING_EVENTS: usize = 1024;

/// QUIC handshake deadline.
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);
/// Deadline for the peer to grant stream credit on `open`.
const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
/// Deadline for the kind byte of a peer-opened stream.
const ACCEPT_KIND_TIMEOUT: Duration = Duration::from_secs(5);
// Same values as a PolkaJAM node: the client keeps the connection alive.
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(7);
const BACKPRESSURE_POLL: Duration = Duration::from_millis(5);
/// How long a dropped transport lets its connections finish closing.
const CLOSE_GRACE: Duration = Duration::from_secs(1);

/// Everything `dial` needs; mirrors the TrUAPI request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Dial {
    pub(super) genesis: [u8; 32],
    /// IPv6 or v4-mapped IPv6.
    pub(super) ip: [u8; 16],
    pub(super) port: u16,
    /// Ed25519 key the peer certificate must carry.
    pub(super) ed25519: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum DialError {
    #[error("peer refused the connection or presented another identity")]
    Refused,
    #[error("connection cap exhausted")]
    Limit,
    #[error("peer unreachable")]
    Unreachable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum OpenError {
    #[error("connection closed or unknown")]
    Closed,
    #[error("stream cap exhausted")]
    Limit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum SendError {
    #[error("stream closed, finished or unknown")]
    Closed,
    #[error("message exceeds the message cap")]
    TooLarge,
    #[error("connection buffer full")]
    Limit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("stream or connection unknown, or fully consumed")]
pub(super) struct Closed;

/// Result of a non-blocking `recv`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Received {
    /// One complete message, or `None` when nothing has arrived yet.
    pub(super) message: Option<Vec<u8>>,
    /// The peer finished its send side and every message has been delivered.
    pub(super) fin: bool,
    /// The peer reset the stream (or sent unframeable data); nothing more
    /// will arrive. Reported after any complete messages.
    pub(super) reset: bool,
}

struct Outgoing {
    bytes: Vec<u8>,
    fin: bool,
}

struct Conn {
    quic: quinn::Connection,
    streams: Vec<u32>,
    buffered: Arc<AtomicUsize>,
    closed: bool,
    task: JoinHandle<()>,
}

struct Stream {
    conn: u32,
    inbox: VecDeque<Vec<u8>>,
    fin: bool,
    reset: bool,
    send_open: bool,
    /// `recv` reported the end of the receive side.
    consumed: bool,
    tx: mpsc::UnboundedSender<Outgoing>,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}

#[derive(Default)]
struct Inner {
    conns: HashMap<u32, Conn>,
    streams: HashMap<u32, Stream>,
    events: VecDeque<latest::JamPeerTransportEvent>,
    next_id: u32,
    /// Dials between the cap check and registration; they hold a slot so
    /// concurrent dials cannot exceed `MAX_CONNECTIONS`.
    dialing: usize,
}

impl Inner {
    fn allocate(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    fn push_event(&mut self, event: latest::JamPeerTransportEvent) {
        if self.events.len() < MAX_PENDING_EVENTS {
            self.events.push_back(event);
        }
    }

    /// Drop a stream, aborting both directions when `abort`; a finished
    /// writer is left to flush.
    fn forget_stream(&mut self, stream: u32, abort: bool) {
        let Some(entry) = self.streams.remove(&stream) else {
            return;
        };
        if abort {
            entry.reader.abort();
            entry.writer.abort();
        }
        if let Some(conn) = self.conns.get_mut(&entry.conn) {
            conn.streams.retain(|&id| id != stream);
            conn.buffered.fetch_sub(
                entry.inbox.iter().map(Vec::len).sum::<usize>(),
                Ordering::AcqRel,
            );
        }
    }
}

type Shared = Arc<Mutex<Inner>>;

/// A reserved connection slot; released on drop unless the dial registered.
struct DialSlot<'a> {
    shared: &'a Shared,
    armed: bool,
}

impl DialSlot<'_> {
    fn commit(mut self, inner: &mut Inner) {
        inner.dialing -= 1;
        self.armed = false;
    }
}

impl Drop for DialSlot<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.shared.lock().dialing -= 1;
        }
    }
}

/// Failure to bring up the endpoint.
#[derive(Debug, thiserror::Error)]
pub(super) enum TransportError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("cannot start the transport runtime: {0}")]
    Runtime(std::io::Error),
    #[error("cannot bind the QUIC endpoint: {0}")]
    Bind(std::io::Error),
}

/// One execution's JAMNP-S client.
pub(super) struct Transport {
    /// Taken on drop and shut down in the background, so the last owner may
    /// release the transport from inside any async context.
    runtime: Option<tokio::runtime::Runtime>,
    endpoint: quinn::Endpoint,
    identity: Identity,
    shared: Shared,
    client_transport: Arc<quinn::TransportConfig>,
}

impl Transport {
    /// Generate an identity, bind an ephemeral dual-stack UDP port and start
    /// the driver runtime.
    pub(super) fn new() -> Result<Self, TransportError> {
        let identity = Identity::generate()?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("jam-peer-transport")
            .enable_all()
            .build()
            .map_err(TransportError::Runtime)?;
        let endpoint = {
            let _guard = runtime.enter();
            quinn::Endpoint::client(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)))
                .map_err(TransportError::Bind)?
        };
        tracing::debug!(
            identity = %peer_id::ed25519_text(identity.public()),
            local = ?endpoint.local_addr().ok(),
            "JAMNP-S endpoint bound",
        );
        let mut client_transport = quinn::TransportConfig::default();
        client_transport.max_idle_timeout(Some(
            IDLE_TIMEOUT.try_into().expect("idle timeout fits a VarInt"),
        ));
        client_transport.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
        Ok(Self {
            runtime: Some(runtime),
            endpoint,
            identity,
            shared: Shared::default(),
            client_transport: Arc::new(client_transport),
        })
    }

    fn runtime(&self) -> &tokio::runtime::Runtime {
        self.runtime
            .as_ref()
            .expect("the runtime lives until the transport drops")
    }

    /// Run a future on the driver runtime and await it from any executor,
    /// waiting at most `timeout`; `None` on timeout or runtime shutdown. The
    /// deadline is armed on the driver runtime: the caller may have no timer.
    async fn run<T: Send + 'static>(
        &self,
        timeout: Duration,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Option<T> {
        self.runtime()
            .spawn(async move { tokio::time::timeout(timeout, future).await.ok() })
            .await
            .ok()
            .flatten()
    }

    /// Connect to one peer, requiring its certificate to carry `ed25519`,
    /// within [`DIAL_TIMEOUT`].
    pub(super) async fn dial(&self, dial: &Dial) -> Result<u32, DialError> {
        let (slot, connecting) = self.connecting(dial)?;
        self.connected(slot, self.run(DIAL_TIMEOUT, connecting).await)
    }

    fn connecting(&self, dial: &Dial) -> Result<(DialSlot<'_>, quinn::Connecting), DialError> {
        let slot = {
            let mut inner = self.shared.lock();
            let open = inner.conns.values().filter(|conn| !conn.closed).count();
            if open + inner.dialing >= MAX_CONNECTIONS {
                return Err(DialError::Limit);
            }
            inner.dialing += 1;
            DialSlot {
                shared: &self.shared,
                armed: true,
            }
        };
        let alpn = super::alpn(&dial.genesis).into_bytes();
        let tls = tls::client_config(&self.identity, dial.ed25519, alpn)
            .map_err(|_| DialError::Refused)?;
        let quic_tls: quinn::crypto::rustls::QuicClientConfig =
            tls.try_into().map_err(|_| DialError::Refused)?;
        let mut config = quinn::ClientConfig::new(Arc::new(quic_tls));
        config.transport_config(self.client_transport.clone());
        let addr = SocketAddr::from((Ipv6Addr::from(dial.ip), dial.port));
        let name = peer_id::ed25519_text(&dial.ed25519);
        // quinn spawns the connection driver from `connect_with`.
        let connecting = {
            let _guard = self.runtime().enter();
            self.endpoint.connect_with(config, addr, &name)
        };
        let connecting = connecting.map_err(|_| DialError::Unreachable)?;
        Ok((slot, connecting))
    }

    fn connected(
        &self,
        slot: DialSlot<'_>,
        outcome: Option<Result<quinn::Connection, quinn::ConnectionError>>,
    ) -> Result<u32, DialError> {
        let connection = match outcome {
            Some(Ok(connection)) => connection,
            // Any TLS alert (QUIC crypto error 0x100–0x1ff) means the peer
            // answered but the handshake failed: a certificate that does not
            // carry the pinned key, a foreign ALPN, or a rejected client.
            Some(Err(quinn::ConnectionError::TransportError(error)))
                if (0x100..0x200).contains(&u64::from(error.code)) =>
            {
                return Err(DialError::Refused);
            }
            Some(Err(
                quinn::ConnectionError::ConnectionClosed(_)
                | quinn::ConnectionError::ApplicationClosed(_),
            )) => return Err(DialError::Refused),
            Some(Err(_)) | None => return Err(DialError::Unreachable),
        };
        let mut inner = self.shared.lock();
        slot.commit(&mut inner);
        let conn = inner.allocate();
        let buffered = Arc::new(AtomicUsize::new(0));
        let task = self.runtime().spawn(accept_loop(
            self.shared.clone(),
            conn,
            connection.clone(),
            buffered.clone(),
        ));
        inner.conns.insert(
            conn,
            Conn {
                quic: connection,
                streams: Vec::new(),
                buffered,
                closed: false,
                task,
            },
        );
        Ok(conn)
    }

    /// Open a bidirectional stream and send its kind byte, within
    /// [`OPEN_TIMEOUT`].
    pub(super) async fn open(&self, conn: u32, kind: u8) -> Result<u32, OpenError> {
        let (quic, buffered) = self.open_target(conn)?;
        let opened = self
            .run(OPEN_TIMEOUT, async move { quic.open_bi().await })
            .await;
        self.opened(conn, kind, buffered, opened)
    }

    fn open_target(&self, conn: u32) -> Result<(quinn::Connection, Arc<AtomicUsize>), OpenError> {
        let inner = self.shared.lock();
        let entry = inner.conns.get(&conn).ok_or(OpenError::Closed)?;
        if entry.closed {
            return Err(OpenError::Closed);
        }
        if entry.streams.len() >= MAX_STREAMS_PER_CONNECTION {
            return Err(OpenError::Limit);
        }
        Ok((entry.quic.clone(), entry.buffered.clone()))
    }

    fn opened(
        &self,
        conn: u32,
        kind: u8,
        buffered: Arc<AtomicUsize>,
        opened: Option<Result<(quinn::SendStream, quinn::RecvStream), quinn::ConnectionError>>,
    ) -> Result<u32, OpenError> {
        let (send, recv) = match opened {
            Some(Ok(pair)) => pair,
            Some(Err(_)) => return Err(OpenError::Closed),
            None => return Err(OpenError::Limit),
        };
        let mut inner = self.shared.lock();
        let Some(entry) = inner.conns.get(&conn).filter(|entry| !entry.closed) else {
            return Err(OpenError::Closed);
        };
        if entry.streams.len() >= MAX_STREAMS_PER_CONNECTION {
            return Err(OpenError::Limit);
        }
        // The writer subtracts every byte it flushes, so count the kind byte.
        buffered.fetch_add(1, Ordering::AcqRel);
        let stream = register_stream(
            self.runtime().handle(),
            &self.shared,
            &mut inner,
            conn,
            send,
            recv,
            buffered,
        );
        let entry = inner.streams.get(&stream).expect("just registered");
        let _ = entry.tx.send(Outgoing {
            bytes: vec![kind],
            fin: false,
        });
        Ok(stream)
    }

    /// Queue one framed message; `fin` finishes the send side after it.
    pub(super) fn send(&self, stream: u32, message: &[u8], fin: bool) -> Result<(), SendError> {
        if message.len() > MAX_MESSAGE_BYTES {
            return Err(SendError::TooLarge);
        }
        let mut inner = self.shared.lock();
        let entry = inner.streams.get(&stream).ok_or(SendError::Closed)?;
        if !entry.send_open {
            return Err(SendError::Closed);
        }
        let buffered = inner
            .conns
            .get(&entry.conn)
            .map(|conn| conn.buffered.clone())
            .ok_or(SendError::Closed)?;
        let framed_len = message.len() + 4;
        if buffered.load(Ordering::Acquire) + framed_len > MAX_BUFFERED_BYTES_PER_CONNECTION {
            return Err(SendError::Limit);
        }
        buffered.fetch_add(framed_len, Ordering::AcqRel);
        let mut bytes = Vec::with_capacity(framed_len);
        bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
        bytes.extend_from_slice(message);
        let entry = inner.streams.get_mut(&stream).expect("checked above");
        if fin {
            entry.send_open = false;
        }
        entry
            .tx
            .send(Outgoing { bytes, fin })
            .map_err(|_| SendError::Closed)
    }

    /// Pop one complete message if any; report fin or reset once drained.
    ///
    /// The call that reports the end with no message consumes the receive
    /// side: later calls are [`Closed`], and a stream whose send side is also
    /// done is forgotten.
    pub(super) fn recv(&self, stream: u32, max: usize) -> Result<Received, Closed> {
        let mut inner = self.shared.lock();
        let entry = inner
            .streams
            .get_mut(&stream)
            .filter(|entry| !entry.consumed)
            .ok_or(Closed)?;
        let mut released = 0;
        if entry.inbox.front().is_some_and(|front| front.len() > max) {
            // The guest cannot take this message; the stream cannot progress.
            released = entry.inbox.drain(..).map(|message| message.len()).sum();
            entry.reset = true;
            entry.reader.abort();
        }
        let message = entry.inbox.pop_front();
        let drained = entry.inbox.is_empty();
        let received = Received {
            fin: drained && entry.fin,
            reset: drained && entry.reset,
            message,
        };
        entry.consumed = received.message.is_none() && (received.fin || received.reset);
        let forget = entry.consumed && (received.reset || !entry.send_open);
        let conn = entry.conn;
        released += received.message.as_ref().map_or(0, Vec::len);
        if let Some(conn) = inner.conns.get(&conn) {
            conn.buffered.fetch_sub(released, Ordering::AcqRel);
        }
        if forget {
            inner.forget_stream(stream, received.reset);
        }
        Ok(received)
    }

    /// Abort both directions and forget the stream.
    pub(super) fn reset(&self, stream: u32) -> Result<(), Closed> {
        let mut inner = self.shared.lock();
        if !inner.streams.contains_key(&stream) {
            return Err(Closed);
        }
        inner.forget_stream(stream, true);
        Ok(())
    }

    /// Close a connection and every stream on it. No `ConnClosed` event is
    /// queued for a guest-initiated close.
    pub(super) fn close(&self, conn: u32) -> Result<(), Closed> {
        let mut inner = self.shared.lock();
        let entry = inner.conns.remove(&conn).ok_or(Closed)?;
        entry.task.abort();
        for stream in entry.streams {
            if let Some(stream) = inner.streams.remove(&stream) {
                stream.reader.abort();
                stream.writer.abort();
            }
        }
        entry.quic.close(0u32.into(), b"");
        Ok(())
    }

    /// Drain pending events in arrival order.
    pub(super) fn events(&self) -> Vec<latest::JamPeerTransportEvent> {
        self.shared.lock().events.drain(..).collect()
    }

    /// Close every connection and the endpoint.
    pub(super) fn shutdown(&self) {
        let conns: Vec<u32> = self.shared.lock().conns.keys().copied().collect();
        for conn in conns {
            let _ = self.close(conn);
        }
        self.endpoint.close(0u32.into(), b"");
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.shutdown();
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        // A validator keeps a connection that was never closed until it idles
        // out, and may refuse this address's next dials meanwhile, so the
        // driver gets to send the close frames. It runs on its own thread:
        // dropping a runtime blocks on its workers, which panics inside
        // another runtime.
        let endpoint = self.endpoint.clone();
        let (handoff, handed) = std::sync::mpsc::channel::<tokio::runtime::Runtime>();
        let closer = std::thread::Builder::new()
            .name("jam-peer-transport-close".into())
            .spawn(move || {
                if let Ok(runtime) = handed.recv() {
                    runtime.block_on(async {
                        let _ = tokio::time::timeout(CLOSE_GRACE, endpoint.wait_idle()).await;
                    });
                }
            });
        let unsent = match closer {
            Ok(_) => handoff.send(runtime).err().map(|unsent| unsent.0),
            Err(_) => Some(runtime),
        };
        if let Some(runtime) = unsent {
            runtime.shutdown_background();
        }
    }
}

fn register_stream(
    handle: &tokio::runtime::Handle,
    shared: &Shared,
    inner: &mut Inner,
    conn: u32,
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    buffered: Arc<AtomicUsize>,
) -> u32 {
    let stream = inner.allocate();
    let (tx, rx) = mpsc::unbounded_channel();
    let reader = handle.spawn(read_loop(shared.clone(), stream, recv, buffered.clone()));
    let writer = handle.spawn(write_loop(shared.clone(), stream, send, rx, buffered));
    inner.streams.insert(
        stream,
        Stream {
            conn,
            inbox: VecDeque::new(),
            fin: false,
            reset: false,
            send_open: true,
            consumed: false,
            tx,
            reader,
            writer,
        },
    );
    inner
        .conns
        .get_mut(&conn)
        .expect("caller checked the connection")
        .streams
        .push(stream);
    stream
}

async fn accept_loop(
    shared: Shared,
    conn: u32,
    quic: quinn::Connection,
    buffered: Arc<AtomicUsize>,
) {
    loop {
        match quic.accept_bi().await {
            Ok((send, mut recv)) => {
                let mut kind = [0u8; 1];
                let read = tokio::time::timeout(ACCEPT_KIND_TIMEOUT, recv.read_exact(&mut kind));
                if !matches!(read.await, Ok(Ok(()))) {
                    // Dropping both halves resets the stream.
                    continue;
                }
                let mut inner = shared.lock();
                let Some(entry) = inner.conns.get(&conn).filter(|entry| !entry.closed) else {
                    return;
                };
                if entry.streams.len() >= MAX_STREAMS_PER_CONNECTION {
                    continue;
                }
                let stream = register_stream(
                    &tokio::runtime::Handle::current(),
                    &shared,
                    &mut inner,
                    conn,
                    send,
                    recv,
                    buffered.clone(),
                );
                inner.push_event(latest::JamPeerTransportEvent::Accepted {
                    conn,
                    stream,
                    kind: kind[0],
                });
            }
            Err(error) => {
                tracing::debug!("JAM peer connection {conn} closed: {error}");
                let mut inner = shared.lock();
                if let Some(entry) = inner.conns.get_mut(&conn) {
                    entry.closed = true;
                    inner.push_event(latest::JamPeerTransportEvent::ConnClosed { conn });
                }
                return;
            }
        }
    }
}

async fn read_loop(
    shared: Shared,
    stream: u32,
    mut recv: quinn::RecvStream,
    buffered: Arc<AtomicUsize>,
) {
    use quinn::ReadExactError;
    loop {
        while buffered.load(Ordering::Acquire) >= MAX_BUFFERED_BYTES_PER_CONNECTION {
            tokio::time::sleep(BACKPRESSURE_POLL).await;
        }
        let mut len = [0u8; 4];
        let clean_fin = match recv.read_exact(&mut len).await {
            Ok(()) => None,
            Err(ReadExactError::FinishedEarly(0)) => Some(true),
            Err(_) => Some(false),
        };
        if let Some(clean) = clean_fin {
            finish_read(&shared, stream, clean);
            return;
        }
        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_MESSAGE_BYTES {
            let _ = recv.stop(0u32.into());
            finish_read(&shared, stream, false);
            return;
        }
        let mut message = vec![0u8; len];
        if recv.read_exact(&mut message).await.is_err() {
            finish_read(&shared, stream, false);
            return;
        }
        buffered.fetch_add(len, Ordering::AcqRel);
        let mut inner = shared.lock();
        match inner.streams.get_mut(&stream) {
            Some(entry) => entry.inbox.push_back(message),
            None => return,
        }
    }
}

fn finish_read(shared: &Shared, stream: u32, clean: bool) {
    let mut inner = shared.lock();
    if let Some(entry) = inner.streams.get_mut(&stream) {
        if clean {
            entry.fin = true;
            inner.push_event(latest::JamPeerTransportEvent::StreamFin { stream });
        } else {
            entry.reset = true;
        }
    }
}

async fn write_loop(
    shared: Shared,
    stream: u32,
    mut send: quinn::SendStream,
    mut rx: mpsc::UnboundedReceiver<Outgoing>,
    buffered: Arc<AtomicUsize>,
) {
    while let Some(outgoing) = rx.recv().await {
        let written = send.write_all(&outgoing.bytes).await;
        buffered.fetch_sub(outgoing.bytes.len(), Ordering::AcqRel);
        if written.is_err() {
            if let Some(entry) = shared.lock().streams.get_mut(&stream) {
                entry.send_open = false;
            }
            return;
        }
        if outgoing.fin {
            let _ = send.finish();
            // Keep the handle until the peer acknowledges or the stream is
            // dropped by `reset`/`close`; dropping early would not reset a
            // finished stream but would forfeit the stopped notification.
            let _ = send.stopped().await;
            return;
        }
    }
    // Channel closed: the stream was reset or its connection closed. Dropping
    // an unfinished SendStream resets it.
}
