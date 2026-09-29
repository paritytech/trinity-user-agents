//! `JamPeerTransport` through a native product runtime against the public JAM
//! devnet (`jam-public-devnet`, six validators on 51.159.188.61).
//!
//! Every call is a product frame: through the generated dispatcher into
//! `ProductRuntimeHost`, whose dial asks the platform for
//! `RemotePermission::JamPeers` and then speaks JAMNP-S QUIC to the validator
//! the frame names.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parity_scale_codec::{Decode, Encode};
use truapi::versioned::jam_peer_transport as wire;
use truapi::{CallError, latest};
use truapi_platform::ProductContext;
use truapi_platform::mock::{MockPlatform, PermissionKind};
use truapi_server::frame::{MESSAGE_TYPE_REQUEST, Payload, ProtocolMessage, request_ids};
use truapi_server::{FrameSink, PairingHostRuntime, ProductRuntime};

// Shared harness; this binary uses only part of it.
#[allow(dead_code)]
mod common;
use common::{test_runtime_config, test_spawner};

const GENESIS: &str = "10c123f02eb6df4c01397d797a112055be691883baa2e82f83b618ed6ce45e46";
/// `follow.json` slot timing of the devnet.
const SLOT_EPOCH_UNIX_MS: u64 = 1_735_732_800_000;
const SLOT_DURATION_MS: u64 = 6_000;
/// Validator UDP ports on 51.159.188.61 and the Ed25519 keys their
/// certificates carry, from the devnet's `bootnodes.json`.
const BOOTNODES: [(u16, &str); 6] = [
    (
        43000,
        "5f7eac6e6a73897aca3ddcc99b763664b9c7e8474d0549d22c553bca8a3d4570",
    ),
    (
        43001,
        "89a6ecb6e586291ef811d8cecb338e2a61ab2f70ed62c94e2f80979d202f1250",
    ),
    (
        43002,
        "273bfffaf8201d658547cd718953c613088ab4d1ee2db448dc481bf165e71ce7",
    ),
    (
        43003,
        "976b529948b1f855ebf146a1254d126eefa4d2ceafa1f9c3328adefdca4de71e",
    ),
    (
        43004,
        "f6be75919e06d1b5ae290a9320c5ece980b42c9d128c2e6f522f16eb77daab96",
    ),
    (
        43005,
        "b22bc9fd19b4dfb2ed0bf1a550cdba3da9afc35323081ea6a456da76bf1c6bef",
    ),
];
const UP_BLOCK_ANNOUNCEMENT: u8 = 0;

/// The tests below compare the process's UDP sockets, so they must not run
/// side by side.
static SERIAL: Mutex<()> = Mutex::new(());

fn bytes32(hex: &str) -> [u8; 32] {
    hex::decode(hex).unwrap().try_into().unwrap()
}

fn dial_request(port: u16, ed25519: &str) -> wire::HostJamPeerTransportDialRequest {
    wire::HostJamPeerTransportDialRequest::V1(latest::HostJamPeerTransportDialRequest {
        genesis: bytes32(GENESIS),
        ip: std::net::Ipv4Addr::new(51, 159, 188, 61)
            .to_ipv6_mapped()
            .octets(),
        port,
        ed25519: bytes32(ed25519),
        p256: None,
    })
}

type Dialed =
    Result<wire::HostJamPeerTransportDialResponse, CallError<wire::HostJamPeerTransportDialError>>;

/// Keeps each response by request id.
#[derive(Default)]
struct Responses(Mutex<HashMap<String, Vec<u8>>>);

impl FrameSink for Responses {
    fn emit_frame(&self, frame: Vec<u8>) {
        let message = ProtocolMessage::decode(&mut &frame[..]).expect("decode emitted frame");
        self.0
            .lock()
            .unwrap()
            .insert(message.request_id, message.payload.value);
    }
}

/// One product connection driven by frames.
struct Product {
    runtime: ProductRuntime,
    responses: Arc<Responses>,
    next_id: AtomicU64,
}

impl Product {
    fn open(platform: Arc<MockPlatform>) -> Self {
        let (host_config, _) = test_runtime_config();
        let host = PairingHostRuntime::new(platform, host_config, test_spawner());
        let responses = Arc::new(Responses::default());
        let product = ProductContext::new("jam.paseo".to_string()).unwrap();
        Self {
            runtime: host.product_runtime(product, responses.clone()),
            responses,
            next_id: AtomicU64::new(0),
        }
    }

    /// Send one request frame and decode its response.
    fn call<Response: Decode>(&self, method: &str, request: impl Encode) -> Response {
        let ids = request_ids(method).expect("registered method");
        let request_id = format!("p:{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let frame = ProtocolMessage {
            request_id: request_id.clone(),
            payload: Payload {
                trait_id: ids.trait_id,
                method_id: ids.method_id,
                message_type: MESSAGE_TYPE_REQUEST,
                value: request.encode(),
            },
        };
        futures::executor::block_on(self.runtime.receive_frame(frame.encode()))
            .expect("the dispatcher accepts the frame");
        let value = self
            .responses
            .0
            .lock()
            .unwrap()
            .remove(&request_id)
            .expect("a request is answered before its dispatch returns");
        Response::decode(&mut &value[..]).expect("decode response")
    }

    /// Dial every bootnode at once, as a light client does, trying a
    /// refused validator up to `attempts` times: a busy validator refuses
    /// some connections and a light client dials it again.
    fn dial_all(&self, attempts: u32) -> Vec<Dialed> {
        let refused = CallError::Domain(wire::HostJamPeerTransportDialError::V1(
            latest::HostJamPeerTransportDialError::Refused,
        ));
        std::thread::scope(|scope| {
            let dials: Vec<_> = BOOTNODES
                .iter()
                .map(|(port, ed25519)| {
                    let refused = &refused;
                    scope.spawn(move || {
                        let mut attempt = 1;
                        loop {
                            let dial: Dialed =
                                self.call("jam_peer_transport_dial", dial_request(*port, ed25519));
                            if dial.as_ref().err() != Some(refused) || attempt == attempts {
                                return dial;
                            }
                            eprintln!("validator :{port} refused attempt {attempt}; retrying");
                            attempt += 1;
                            std::thread::sleep(Duration::from_secs(3));
                        }
                    })
                })
                .collect();
            dials.into_iter().map(|dial| dial.join().unwrap()).collect()
        })
    }

    fn recv(&self, stream: u32) -> latest::HostJamPeerTransportRecvResponse {
        let response: Result<
            wire::HostJamPeerTransportRecvResponse,
            CallError<wire::HostJamPeerTransportRecvError>,
        > = self.call(
            "jam_peer_transport_recv",
            wire::HostJamPeerTransportRecvRequest::V1(latest::HostJamPeerTransportRecvRequest {
                stream,
                max: latest::JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES,
            }),
        );
        let wire::HostJamPeerTransportRecvResponse::V1(response) =
            response.expect("the stream stays readable");
        response
    }
}

/// UDP sockets this process holds, by inode.
#[cfg(target_os = "linux")]
fn udp_sockets() -> BTreeSet<u64> {
    let held: BTreeSet<u64> = std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
        .filter_map(|target| {
            let target = target.to_str()?;
            target
                .strip_prefix("socket:[")?
                .strip_suffix(']')?
                .parse()
                .ok()
        })
        .collect();
    ["/proc/self/net/udp", "/proc/self/net/udp6"]
        .iter()
        .flat_map(|table| {
            std::fs::read_to_string(table)
                .unwrap_or_default()
                .lines()
                .skip(1)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter_map(|line| line.split_whitespace().nth(9)?.parse().ok())
        .filter(|inode| held.contains(inode))
        .collect()
}

fn blake2b_256(bytes: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .hash(bytes)
        .as_bytes()
        .try_into()
        .unwrap()
}

fn slot_now() -> u64 {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    (now_ms - SLOT_EPOCH_UNIX_MS) / SLOT_DURATION_MS
}

/// What one validator told us on UP 0.
#[derive(Debug, Default)]
struct Up0 {
    /// Finalized slot the peer's handshake claims.
    finalized_slot: Option<u32>,
    /// `(slot, header hash)` of each announced block.
    announced: Vec<(u32, [u8; 32])>,
}

#[test]
#[ignore = "needs network access to the public JAM devnet"]
fn dials_every_public_devnet_validator_and_hears_block_announcements() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let platform = Arc::new(MockPlatform::new());
    platform.grant_permission("JamPeers");
    let product = Product::open(platform.clone());
    eprintln!(
        "ALPN {}",
        truapi_server::jam_peer_transport::alpn(&bytes32(GENESIS))
    );

    let started = Instant::now();
    let conns: Vec<u32> = product
        .dial_all(5)
        .into_iter()
        .zip(BOOTNODES)
        .map(|(dial, (port, _))| {
            let wire::HostJamPeerTransportDialResponse::V1(dialed) =
                dial.unwrap_or_else(|error| panic!("dial :{port} failed: {error:?}"));
            dialed.conn
        })
        .collect();
    eprintln!(
        "dialed {} validators in {:?}",
        conns.len(),
        started.elapsed()
    );
    let prompts = platform
        .permission_log()
        .iter()
        .filter(|entry| entry.kind == PermissionKind::Remote)
        .count();

    // UP 0: Handshake = Final ++ len++[Leaf]; Final = Header Hash ++ Slot.
    let mut handshake = bytes32(GENESIS).to_vec();
    handshake.extend_from_slice(&0u32.to_le_bytes());
    handshake.push(0);
    let streams: Vec<u32> = conns
        .iter()
        .map(|&conn| {
            let opened: Result<
                wire::HostJamPeerTransportOpenResponse,
                CallError<wire::HostJamPeerTransportOpenError>,
            > = product.call(
                "jam_peer_transport_open",
                wire::HostJamPeerTransportOpenRequest::V1(
                    latest::HostJamPeerTransportOpenRequest {
                        conn,
                        kind: UP_BLOCK_ANNOUNCEMENT,
                    },
                ),
            );
            let wire::HostJamPeerTransportOpenResponse::V1(opened) = opened.unwrap();
            let sent: Result<
                wire::HostJamPeerTransportSendResponse,
                CallError<wire::HostJamPeerTransportSendError>,
            > = product.call(
                "jam_peer_transport_send",
                wire::HostJamPeerTransportSendRequest::V1(
                    latest::HostJamPeerTransportSendRequest {
                        stream: opened.stream,
                        message: handshake.clone(),
                        fin: false,
                    },
                ),
            );
            sent.unwrap();
            opened.stream
        })
        .collect();

    let mut peers: Vec<Up0> = streams.iter().map(|_| Up0::default()).collect();
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline && peers.iter().any(|peer| peer.announced.is_empty()) {
        for (peer, &stream) in peers.iter_mut().zip(&streams) {
            let received = product.recv(stream);
            assert!(
                !received.fin && !received.reset,
                "UP 0 stays open: {received:?}"
            );
            let Some(message) = received.message else {
                continue;
            };
            if peer.finalized_slot.is_none() {
                peer.finalized_slot = Some(u32::from_le_bytes(message[32..36].try_into().unwrap()));
                continue;
            }
            // Announcement = Header ++ Final; the header's slot follows the
            // parent hash, prior state root and extrinsic hash.
            let header = &message[..message.len() - 36];
            let slot = u32::from_le_bytes(header[96..100].try_into().unwrap());
            peer.announced.push((slot, blake2b_256(header)));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let wall_slot = slot_now();
    for ((port, _), peer) in BOOTNODES.iter().zip(&peers) {
        eprintln!(
            "validator :{port} finalized_slot={:?} announced={:?}",
            peer.finalized_slot,
            peer.announced
                .iter()
                .map(|(slot, hash)| format!("{slot}:{}", hex::encode(&hash[..8])))
                .collect::<Vec<_>>(),
        );
    }
    eprintln!("wall-clock slot {wall_slot}");

    let events: Result<
        wire::HostJamPeerTransportEventsResponse,
        CallError<wire::HostJamPeerTransportEventsError>,
    > = product.call(
        "jam_peer_transport_events",
        wire::HostJamPeerTransportEventsRequest::V1,
    );
    let wire::HostJamPeerTransportEventsResponse::V1(events) = events.unwrap();
    eprintln!("events {:?}", events.events);
    assert!(
        !events
            .events
            .iter()
            .any(|event| matches!(event, latest::JamPeerTransportEvent::ConnClosed { .. })),
        "no validator dropped us: {:?}",
        events.events,
    );

    let live = |peer: &Up0| {
        peer.finalized_slot.is_some()
            && peer.announced.iter().any(|&(slot, _)| {
                (wall_slot.saturating_sub(5)..=wall_slot + 1).contains(&u64::from(slot))
            })
    };
    let blocks: BTreeSet<_> = peers
        .iter()
        .flat_map(|peer| peer.announced.iter())
        .collect();
    let shared_blocks = blocks
        .iter()
        .filter(|&&block| {
            peers
                .iter()
                .filter(|peer| peer.announced.contains(block))
                .count()
                > 1
        })
        .count();
    assert_eq!(
        (
            prompts,
            peers.iter().filter(|peer| live(peer)).count(),
            shared_blocks > 0
        ),
        (1, BOOTNODES.len(), true),
        "one prompt covers every dial; each validator completes the UP 0 handshake and announces \
         a block at the wall-clock slot; validators agree on at least one block",
    );

    for conn in conns {
        let closed: Result<
            wire::HostJamPeerTransportCloseResponse,
            CallError<wire::HostJamPeerTransportCloseError>,
        > = product.call(
            "jam_peer_transport_close",
            wire::HostJamPeerTransportCloseRequest::V1(latest::HostJamPeerTransportCloseRequest {
                conn,
            }),
        );
        closed.unwrap();
    }
    product.runtime.dispose();
}

/// A refused product must not reach the network at all: no dial binds a
/// socket, so no packet can leave for the validators it named.
#[cfg(target_os = "linux")]
#[test]
fn a_refused_product_opens_no_socket_for_any_dial() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let platform = Arc::new(MockPlatform::new());
    platform.revoke_permission("JamPeers");
    let product = Product::open(platform.clone());
    let before = udp_sockets();

    let dials = product.dial_all(1);

    let not_granted = CallError::Domain(wire::HostJamPeerTransportDialError::V1(
        latest::HostJamPeerTransportDialError::NotGranted,
    ));
    assert_eq!(
        (dials, udp_sockets().difference(&before).count()),
        (vec![Err(not_granted); BOOTNODES.len()], 0),
    );
    product.runtime.dispose();
}
