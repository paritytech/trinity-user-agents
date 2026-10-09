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

use futures::stream::{FuturesUnordered, StreamExt};
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

/// A frame owns its quota until delivered, flushed, or dropped on any error.
struct Reservation {
    buffered: Arc<AtomicUsize>,
    bytes: usize,
}

impl Reservation {
    fn new(buffered: &Arc<AtomicUsize>, bytes: usize) -> Option<Self> {
        buffered
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|&total| total <= MAX_BUFFERED_BYTES_PER_CONNECTION)
            })
            .ok()?;
        Some(Self {
            buffered: buffered.clone(),
            bytes,
        })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.buffered.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

struct Incoming {
    bytes: Vec<u8>,
    _reservation: Reservation,
}

struct Outgoing {
    bytes: Vec<u8>,
    fin: bool,
    _reservation: Reservation,
}

struct Conn {
    quic: quinn::Connection,
    streams: Vec<u32>,
    opening: usize,
    buffered: Arc<AtomicUsize>,
    closed: bool,
    task: JoinHandle<()>,
}

struct Stream {
    conn: u32,
    inbox: VecDeque<Incoming>,
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

/// Both local opens waiting for credit and incoming streams awaiting a kind
/// byte hold a slot until they register or are cancelled.
struct StreamSlot {
    shared: Shared,
    conn: u32,
    armed: bool,
}

impl StreamSlot {
    fn reserve(shared: &Shared, inner: &mut Inner, conn: u32) -> Result<Self, OpenError> {
        let entry = inner.conns.get_mut(&conn).filter(|entry| !entry.closed)
            .ok_or(OpenError::Closed)?;
        if entry.streams.len() + entry.opening >= MAX_STREAMS_PER_CONNECTION {
            return Err(OpenError::Limit);
        }
        entry.opening += 1;
        Ok(Self { shared: shared.clone(), conn, armed: true })
    }

    fn commit(mut self, inner: &mut Inner) {
        if let Some(entry) = inner.conns.get_mut(&self.conn) {
            entry.opening -= 1;
        }
        self.armed = false;
    }
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        if self.armed && let Some(entry) = self.shared.lock().conns.get_mut(&self.conn) {
            entry.opening -= 1;
        }
    }
}

/// Dropping an awaiting call must cancel, not detach, its driver task.
struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Quinn implicitly finishes a dropped sender; cancellation must reset it.
struct ResetOnDrop(quinn::SendStream);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        let _ = self.0.reset(0u32.into());
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
        };
        let endpoint = match endpoint {
            Ok(endpoint) => endpoint,
            Err(error) => {
                // Construction may be called from an async runtime too.
                runtime.shutdown_background();
                return Err(TransportError::Bind(error));
            }
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
        let mut task = AbortOnDrop(self.runtime().spawn(async move {
            tokio::time::timeout(timeout, future).await.ok()
        }));
        (&mut task.0)
            .await
            .ok()
            .flatten()
    }

    /// Count permission-waiting dials against retained connection handles.
    /// Keep the connection lock through reservation so a completing handshake
    /// cannot fall between the connection and pending-count snapshots.
    pub(super) fn reserve_pending_dial(&self, pending: &AtomicUsize) -> bool {
        let inner = self.shared.lock();
        pending
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (inner.conns.len() + used < MAX_CONNECTIONS).then_some(used + 1)
            })
            .is_ok()
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
            if inner.conns.len() + inner.dialing >= MAX_CONNECTIONS {
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
                opening: 0,
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
        let (slot, quic, buffered) = self.open_target(conn)?;
        let opened = self
            .run(OPEN_TIMEOUT, async move {
                quic.open_bi()
                    .await
                    .map(|(send, recv)| (ResetOnDrop(send), recv))
            })
            .await;
        self.opened(slot, conn, kind, buffered, opened)
    }

    fn open_target(&self, conn: u32) -> Result<(StreamSlot, quinn::Connection, Arc<AtomicUsize>), OpenError> {
        let mut inner = self.shared.lock();
        let slot = StreamSlot::reserve(&self.shared, &mut inner, conn)?;
        let entry = &inner.conns[&conn];
        Ok((slot, entry.quic.clone(), entry.buffered.clone()))
    }

    fn opened(
        &self,
        slot: StreamSlot,
        conn: u32,
        kind: u8,
        buffered: Arc<AtomicUsize>,
        opened: Option<Result<(ResetOnDrop, quinn::RecvStream), quinn::ConnectionError>>,
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
        let reservation = Reservation::new(&buffered, 1).ok_or(OpenError::Limit)?;
        slot.commit(&mut inner);
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
            _reservation: reservation,
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
        let reservation = Reservation::new(&buffered, framed_len).ok_or(SendError::Limit)?;
        let mut bytes = Vec::with_capacity(framed_len);
        bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
        bytes.extend_from_slice(message);
        let entry = inner.streams.get_mut(&stream).expect("checked above");
        if fin {
            entry.send_open = false;
        }
        entry
            .tx
            .send(Outgoing {
                bytes,
                fin,
                _reservation: reservation,
            })
            .map_err(|_| SendError::Closed)?;
        if fin && entry.consumed {
            inner.forget_stream(stream, false);
        }
        Ok(())
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
        if entry.inbox.front().is_some_and(|front| front.bytes.len() > max) {
            // The guest cannot take this message; the stream cannot progress.
            entry.inbox.clear();
            entry.reset = true;
            entry.reader.abort();
        }
        let message = entry.inbox.pop_front().map(|incoming| incoming.bytes);
        let drained = entry.inbox.is_empty();
        let received = Received {
            fin: drained && entry.fin,
            reset: drained && entry.reset,
            message,
        };
        entry.consumed = received.message.is_none() && (received.fin || received.reset);
        let forget = entry.consumed && (received.reset || !entry.send_open);
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
    send: ResetOnDrop,
    recv: quinn::RecvStream,
    buffered: Arc<AtomicUsize>,
) -> u32 {
    let stream = inner.allocate();
    let (tx, rx) = mpsc::unbounded_channel();
    let reader = handle.spawn(read_loop(shared.clone(), stream, recv, buffered));
    let writer = handle.spawn(write_loop(shared.clone(), stream, send, rx));
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
    let mut accepting = FuturesUnordered::new();
    loop {
        tokio::select! {
            _ = accepting.next(), if !accepting.is_empty() => {}
            incoming = quic.accept_bi() => match incoming {
                Ok((send, recv)) => {
                    let send = ResetOnDrop(send);
                    let slot = {
                        let mut inner = shared.lock();
                        if inner.events.len() >= MAX_PENDING_EVENTS {
                            continue;
                        }
                        match StreamSlot::reserve(&shared, &mut inner, conn) {
                            Ok(slot) => slot,
                            Err(OpenError::Closed) => return,
                            Err(OpenError::Limit) => continue,
                        }
                    };
                    accepting.push(accept_stream(
                        shared.clone(), slot, conn, send, recv, buffered.clone(),
                    ));
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
}

async fn accept_stream(
    shared: Shared,
    slot: StreamSlot,
    conn: u32,
    send: ResetOnDrop,
    mut recv: quinn::RecvStream,
    buffered: Arc<AtomicUsize>,
) {
    let mut kind = [0u8; 1];
    let read = tokio::time::timeout(ACCEPT_KIND_TIMEOUT, recv.read_exact(&mut kind));
    if !matches!(read.await, Ok(Ok(()))) {
        return;
    }
    let mut inner = shared.lock();
    if !inner.conns.get(&conn).is_some_and(|entry| !entry.closed)
        || inner.events.len() >= MAX_PENDING_EVENTS
    {
        return;
    }
    slot.commit(&mut inner);
    let stream = register_stream(
        &tokio::runtime::Handle::current(),
        &shared,
        &mut inner,
        conn,
        send,
        recv,
        buffered,
    );
    inner.push_event(latest::JamPeerTransportEvent::Accepted {
        conn,
        stream,
        kind: kind[0],
    });
}

async fn read_loop(
    shared: Shared,
    stream: u32,
    mut recv: quinn::RecvStream,
    buffered: Arc<AtomicUsize>,
) {
    use quinn::ReadExactError;
    loop {
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
        // Reserve before allocating or reading the payload. Include framing
        // so a peer cannot buffer an unbounded number of empty messages.
        let reservation = loop {
            if let Some(reservation) = Reservation::new(&buffered, len + 4) {
                break reservation;
            }
            tokio::time::sleep(BACKPRESSURE_POLL).await;
        };
        let mut message = vec![0u8; len];
        if recv.read_exact(&mut message).await.is_err() {
            finish_read(&shared, stream, false);
            return;
        }
        let mut inner = shared.lock();
        match inner.streams.get_mut(&stream) {
            Some(entry) => entry.inbox.push_back(Incoming {
                bytes: message,
                _reservation: reservation,
            }),
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
    mut send: ResetOnDrop,
    mut rx: mpsc::UnboundedReceiver<Outgoing>,
) {
    while let Some(outgoing) = rx.recv().await {
        let Outgoing { bytes, fin, _reservation } = outgoing;
        let written = send.0.write_all(&bytes).await;
        drop(bytes);
        drop(_reservation);
        if written.is_err() {
            let mut inner = shared.lock();
            if let Some(entry) = inner.streams.get_mut(&stream) {
                entry.send_open = false;
                if entry.consumed {
                    inner.forget_stream(stream, true);
                }
            }
            return;
        }
        if fin {
            let _ = send.0.finish();
            // Retain the reset guard until acknowledged, so reset/close can
            // still abandon buffered transmission after finish.
            let _ = send.0.stopped().await;
            return;
        }
    }
    // The reset guard also aborts the send side when its queue closes.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_bound_empty_frames_and_release_queued_bytes_on_drop() {
        let buffered = Arc::new(AtomicUsize::new(0));
        let held = Reservation::new(&buffered, MAX_BUFFERED_BYTES_PER_CONNECTION - 4).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Outgoing {
            bytes: vec![0; 4],
            fin: false,
            _reservation: Reservation::new(&buffered, 4).unwrap(),
        })
        .ok()
        .unwrap();
        assert!(Reservation::new(&buffered, 4).is_none());
        drop(rx);
        assert_eq!(buffered.load(Ordering::Acquire), MAX_BUFFERED_BYTES_PER_CONNECTION - 4);
        drop(held);
        assert_eq!(buffered.load(Ordering::Acquire), 0);

        let mut inbox = VecDeque::new();
        inbox.push_back(Incoming {
            bytes: Vec::new(),
            _reservation: Reservation::new(&buffered, 4).unwrap(),
        });
        assert_eq!(buffered.load(Ordering::Acquire), 4);
        let message = inbox.pop_front().map(|incoming| incoming.bytes);
        assert_eq!(message, Some(Vec::new()));
        assert_eq!(buffered.load(Ordering::Acquire), 0);
    }

    #[test]
    fn simultaneous_reservations_never_exceed_the_connection_budget() {
        let buffered = Arc::new(AtomicUsize::new(0));
        let barrier = std::sync::Barrier::new(16);
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..16)
                .map(|_| scope.spawn(|| {
                    let reservation = Reservation::new(&buffered, MAX_MESSAGE_BYTES);
                    barrier.wait();
                    let used = buffered.load(Ordering::Acquire);
                    barrier.wait();
                    (reservation.is_some(), used)
                }))
                .collect();
            let results: Vec<_> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
            assert!(results.iter().all(|&(_, used)| used == MAX_BUFFERED_BYTES_PER_CONNECTION));
            assert_eq!(results.iter().filter(|&&(granted, _)| granted).count(), 4);
        });
        assert_eq!(buffered.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn dropping_a_driver_call_cancels_its_work_and_releases_its_reservations() {
        let transport = Transport::new().unwrap();
        let buffered = Arc::new(AtomicUsize::new(0));
        let reservation = Reservation::new(&buffered, 4).unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let mut call = Box::pin(transport.run(Duration::from_secs(60), async move {
            let _reservation = reservation;
            let _ = started.send(());
            std::future::pending::<()>().await;
        }));
        tokio::select! {
            _ = &mut call => panic!("pending work completed"),
            _ = ready => {}
        }
        drop(call);
        tokio::time::timeout(Duration::from_secs(1), async {
            while buffered.load(Ordering::Acquire) != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("dropping the caller aborts its driver task");
    }

    fn server() -> (quinn::Endpoint, Identity) {
        let identity = Identity::generate().unwrap();
        let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(identity.cert_chain(), identity.private_key())
        .unwrap();
        tls.alpn_protocols = vec![super::super::alpn(&[1; 32]).into_bytes()];
        let config = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap(),
        ));
        let endpoint = quinn::Endpoint::server(
            config,
            (std::net::Ipv4Addr::LOCALHOST, 0).into(),
        )
        .unwrap();
        (endpoint, identity)
    }

    async fn connect(
        transport: &Transport,
        server: &quinn::Endpoint,
        identity: &Identity,
    ) -> (u32, quinn::Connection) {
        let request = Dial {
            genesis: [1; 32],
            ip: std::net::Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets(),
            port: server.local_addr().unwrap().port(),
            ed25519: *identity.public(),
        };
        let (client, peer) = tokio::join!(
            transport.dial(&request),
            async { server.accept().await.unwrap().await.unwrap() },
        );
        (client.unwrap(), peer)
    }

    #[tokio::test]
    async fn sending_fin_after_consuming_peer_fin_releases_the_stream_slot() {
        let transport = Transport::new().unwrap();
        let (server, identity) = server();
        let (conn, peer) = connect(&transport, &server, &identity).await;
        let stream = transport.open(conn, 0).await.unwrap();
        let (mut send, mut recv) = peer.accept_bi().await.unwrap();
        let mut kind = [0];
        recv.read_exact(&mut kind).await.unwrap();
        send.finish().unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if transport.recv(stream, 1024).unwrap().fin {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        transport.send(stream, b"done", true).unwrap();
        assert!(!transport.shared.lock().streams.contains_key(&stream));
        let mut framed = [0; 8];
        recv.read_exact(&mut framed).await.unwrap();
        assert_eq!(&framed, b"\x04\0\0\0done");
    }

    #[tokio::test]
    async fn remote_closed_handles_still_consume_connection_slots_until_closed() {
        let transport = Transport::new().unwrap();
        let (server, identity) = server();
        let mut conns = Vec::new();
        for _ in 0..MAX_CONNECTIONS {
            let (conn, peer) = connect(&transport, &server, &identity).await;
            peer.close(0u32.into(), b"");
            tokio::time::timeout(Duration::from_secs(1), async {
                while !transport.shared.lock().conns[&conn].closed {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            conns.push(conn);
        }
        let request = Dial {
            genesis: [1; 32],
            ip: std::net::Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets(),
            port: server.local_addr().unwrap().port(),
            ed25519: *identity.public(),
        };
        assert_eq!(transport.dial(&request).await, Err(DialError::Limit));
        transport.close(conns[0]).unwrap();
        connect(&transport, &server, &identity).await;
    }

    #[tokio::test]
    async fn empty_messages_pause_at_quota_and_reset_releases_buffers_and_resets_peer() {
        let transport = Transport::new().unwrap();
        let (server, identity) = server();
        let (conn, peer) = connect(&transport, &server, &identity).await;
        let stream = transport.open(conn, 0).await.unwrap();
        let (mut send, mut recv) = peer.accept_bi().await.unwrap();
        let mut kind = [0];
        recv.read_exact(&mut kind).await.unwrap();
        let buffered = transport.shared.lock().conns[&conn].buffered.clone();
        tokio::time::timeout(Duration::from_secs(1), async {
            while buffered.load(Ordering::Acquire) != 0 {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        let held = Reservation::new(&buffered, MAX_BUFFERED_BYTES_PER_CONNECTION - 8).unwrap();
        send.write_all(&[0; 12]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while transport.shared.lock().streams[&stream].inbox.len() != 2 {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        assert_eq!(buffered.load(Ordering::Acquire), MAX_BUFFERED_BYTES_PER_CONNECTION);
        assert_eq!(transport.recv(stream, 0).unwrap().message, Some(Vec::new()));
        tokio::time::timeout(Duration::from_secs(1), async {
            while transport.shared.lock().streams[&stream].inbox.len() != 2 {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        transport.reset(stream).unwrap();
        drop(held);
        tokio::time::timeout(Duration::from_secs(1), async {
            while buffered.load(Ordering::Acquire) != 0 {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), recv.read(&mut kind))
            .await
            .unwrap();
        assert!(matches!(result, Err(quinn::ReadError::Reset(_))));
    }

    #[tokio::test]
    async fn an_inbound_stream_is_reset_when_its_handle_cannot_be_announced() {
        let transport = Transport::new().unwrap();
        let (server, identity) = server();
        let (_conn, peer) = connect(&transport, &server, &identity).await;
        {
            let mut inner = transport.shared.lock();
            for _ in 0..MAX_PENDING_EVENTS {
                inner.push_event(latest::JamPeerTransportEvent::StreamFin { stream: 0 });
            }
        }
        let (mut send, mut recv) = peer.open_bi().await.unwrap();
        send.write_all(&[0]).await.unwrap();
        let mut bytes = [0];
        let result = tokio::time::timeout(Duration::from_secs(1), recv.read(&mut bytes))
            .await
            .unwrap();
        assert!(matches!(result, Err(quinn::ReadError::Reset(_))));
        assert!(transport.shared.lock().streams.is_empty());
    }

    #[tokio::test]
    async fn pending_streams_hold_slots_and_dropping_them_releases_credit() {
        let transport = Transport::new().unwrap();
        let (server, identity) = server();
        let (conn, _peer) = connect(&transport, &server, &identity).await;
        let pending: Vec<_> = (0..MAX_STREAMS_PER_CONNECTION)
            .map(|_| transport.open_target(conn).unwrap())
            .collect();
        assert!(matches!(transport.open_target(conn), Err(OpenError::Limit)));
        drop(pending);
        assert!(transport.open_target(conn).is_ok());
        assert_eq!(transport.shared.lock().conns[&conn].opening, 0);
    }

    #[tokio::test]
    async fn an_inbound_stream_without_a_kind_does_not_block_later_streams() {
        let transport = Transport::new().unwrap();
        let (server, identity) = server();
        let (conn, peer) = connect(&transport, &server, &identity).await;
        // Sending on the higher stream id implicitly opens this lower id,
        // but leaves its kind byte unavailable.
        let (_silent_send, _silent_recv) = peer.open_bi().await.unwrap();
        let (mut send, _recv) = peer.open_bi().await.unwrap();
        send.write_all(&[42]).await.unwrap();
        let stream = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                for event in transport.events() {
                    if let latest::JamPeerTransportEvent::Accepted { stream, kind: 42, .. } = event {
                        return stream;
                    }
                }
                tokio::task::yield_now().await;
            }
        }).await.expect("one missing kind byte must not stall another stream");
        assert_eq!(transport.shared.lock().streams[&stream].conn, conn);
    }
}
