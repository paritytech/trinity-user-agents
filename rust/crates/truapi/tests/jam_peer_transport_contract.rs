//! JamPeerTransport contract regression test.
//!
//! Pins the frozen trait-111 wire ids and SCALE layouts, the `JamPeers`
//! permission's place in `RemotePermission`, and the genesis/ALPN helpers a
//! host uses on dial.

use parity_scale_codec::{Decode, Encode};
use truapi::generated::wire_table::{
    JAM_PEER_TRANSPORT_CLOSE, JAM_PEER_TRANSPORT_DIAL, JAM_PEER_TRANSPORT_EVENTS,
    JAM_PEER_TRANSPORT_OPEN, JAM_PEER_TRANSPORT_RECV, JAM_PEER_TRANSPORT_RESET,
    JAM_PEER_TRANSPORT_SEND, MethodIds,
};
use truapi::jam_peer_transport::{InvalidGenesis, alpn, parse_genesis};
use truapi::latest;
use truapi::versioned::jam_peer_transport;

const GENESIS_HEX: &str = "353963b9cedfe4ea22038081052a5c151b06b55a4a026a97522cd0320cabf49f";

fn genesis() -> [u8; 32] {
    parse_genesis(GENESIS_HEX).unwrap()
}

#[test]
fn a_genesis_has_one_spelling_and_names_its_alpn() {
    assert_eq!(parse_genesis(&format!("0x{GENESIS_HEX}")), Ok(genesis()));
    assert_eq!(genesis()[..4], [0x35, 0x39, 0x63, 0xb9]);
    assert_eq!(alpn(&genesis()), "jamnp-s/1/353963b9");
    assert_eq!(alpn(&[0xab; 32]), "jamnp-s/1/abababab");
    for text in [
        GENESIS_HEX[..62].to_string(),
        GENESIS_HEX.to_uppercase(),
        format!("{GENESIS_HEX}0"),
        format!("0X{GENESIS_HEX}"),
        format!("{}zz", &GENESIS_HEX[..62]),
        String::new(),
    ] {
        assert_eq!(parse_genesis(&text), Err(InvalidGenesis), "{text}");
    }
}

/// `JamPeers` is appended last, so every earlier permission keeps the SCALE
/// index stored decisions and older peers already use.
#[test]
fn jam_peers_is_the_last_remote_permission_and_names_its_genesis() {
    for (permission, index) in [
        (
            latest::RemotePermission::Remote {
                domains: Vec::new(),
            },
            0u8,
        ),
        (latest::RemotePermission::WebRtc, 1),
        (latest::RemotePermission::ChainSubmit, 2),
        (latest::RemotePermission::PreimageSubmit, 3),
        (latest::RemotePermission::StatementSubmit, 4),
    ] {
        assert_eq!(permission.encode()[0], index, "{permission:?}");
    }
    let jam = latest::RemotePermission::JamPeers { genesis: genesis() };
    let mut expected = vec![5u8];
    expected.extend_from_slice(&genesis());
    assert_eq!(jam.encode(), expected);
    assert_eq!(
        latest::RemotePermission::decode(&mut &expected[..]),
        Ok(jam.clone())
    );
    assert_eq!(jam.to_string(), "connections to JAM network 0x353963b9…");
}

/// The frozen contract: namespace 111, methods 0..6 in this order, V1 payloads.
#[test]
fn the_wire_ids_and_scale_layout_match_the_frozen_contract() {
    for (ids, method_id) in [
        (JAM_PEER_TRANSPORT_DIAL, 0),
        (JAM_PEER_TRANSPORT_OPEN, 1),
        (JAM_PEER_TRANSPORT_SEND, 2),
        (JAM_PEER_TRANSPORT_RECV, 3),
        (JAM_PEER_TRANSPORT_RESET, 4),
        (JAM_PEER_TRANSPORT_CLOSE, 5),
        (JAM_PEER_TRANSPORT_EVENTS, 6),
    ] {
        assert_eq!(
            ids,
            MethodIds {
                trait_id: 111,
                method_id
            }
        );
    }

    let dial = jam_peer_transport::HostJamPeerTransportDialRequest::V1(
        latest::HostJamPeerTransportDialRequest {
            genesis: genesis(),
            ip: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 127, 0, 0, 1],
            port: 43000,
            ed25519: [0x11; 32],
            p256: Some([0x02; 33]),
        },
    )
    .encode();
    let mut expected = vec![0u8];
    expected.extend_from_slice(&genesis());
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 127, 0, 0, 1]);
    expected.extend_from_slice(&43000u16.to_le_bytes());
    expected.extend_from_slice(&[0x11; 32]);
    expected.push(1);
    expected.extend_from_slice(&[0x02; 33]);
    assert_eq!(dial, expected);

    assert_eq!(
        jam_peer_transport::HostJamPeerTransportSendRequest::V1(
            latest::HostJamPeerTransportSendRequest {
                stream: 7,
                message: vec![0xaa, 0xbb],
                fin: true,
            }
        )
        .encode(),
        vec![0, 7, 0, 0, 0, 8, 0xaa, 0xbb, 1]
    );
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportRecvResponse::V1(
            latest::HostJamPeerTransportRecvResponse {
                message: None,
                fin: false,
                reset: true,
            }
        )
        .encode(),
        vec![0, 0, 0, 1]
    );
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportEventsResponse::V1(
            latest::HostJamPeerTransportEventsResponse {
                events: vec![
                    latest::JamPeerTransportEvent::ConnClosed { conn: 1 },
                    latest::JamPeerTransportEvent::StreamFin { stream: 2 },
                    latest::JamPeerTransportEvent::Accepted {
                        conn: 1,
                        stream: 3,
                        kind: 0,
                    },
                ],
            }
        )
        .encode(),
        vec![
            0, 12, 0, 1, 0, 0, 0, 1, 2, 0, 0, 0, 2, 1, 0, 0, 0, 3, 0, 0, 0, 0
        ]
    );
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportDialError::V1(
            latest::HostJamPeerTransportDialError::Unreachable
        )
        .encode(),
        vec![0, 3]
    );
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportEventsRequest::V1.encode(),
        vec![0]
    );
    assert_eq!(latest::JAM_PEER_TRANSPORT_MAX_CONNECTIONS, 8);
    assert_eq!(latest::JAM_PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION, 16);
    assert_eq!(latest::JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES, 1 << 20);
    assert_eq!(
        latest::JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION,
        4 << 20
    );
}

#[test]
fn listener_and_chunk_wire_golden_vectors() {
    use truapi::generated::wire_table::{
        JAM_PEER_TRANSPORT_LISTEN, JAM_PEER_TRANSPORT_RECV_CHUNK,
        JAM_PEER_TRANSPORT_SEND_CHUNK,
    };
    for (ids, method_id) in [
        (JAM_PEER_TRANSPORT_LISTEN, 7),
        (JAM_PEER_TRANSPORT_SEND_CHUNK, 8),
        (JAM_PEER_TRANSPORT_RECV_CHUNK, 9),
    ] {
        assert_eq!(ids, MethodIds { trait_id: 111, method_id });
    }
    let ip = [0x22; 16];
    let listen = jam_peer_transport::HostJamPeerTransportListenRequest::V1(
        latest::HostJamPeerTransportListenRequest {
            genesis: [0x11; 32], ip, port: 43000, allowed_peers: vec![[0x33; 32]],
        },
    );
    let mut expected = vec![0];
    expected.extend_from_slice(&[0x11; 32]);
    expected.extend_from_slice(&ip);
    expected.extend_from_slice(&[0xf8, 0xa7, 4]);
    expected.extend_from_slice(&[0x33; 32]);
    assert_eq!(listen.encode(), expected);
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportListenRequest::decode(&mut &expected[..]),
        Ok(listen),
    );
    let mut endpoint = vec![0];
    endpoint.extend_from_slice(&ip);
    endpoint.extend_from_slice(&[0xf8, 0xa7]);
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportListenResponse::V1(
            latest::HostJamPeerTransportListenResponse { ip, port: 43000 },
        ).encode(), endpoint,
    );
    let event = latest::JamPeerTransportEvent::PeerAccepted {
        conn: 7, ed25519: [0x33; 32], ip, port: 43000,
    };
    let mut expected = vec![3, 7, 0, 0, 0];
    expected.extend_from_slice(&[0x33; 32]);
    expected.extend_from_slice(&ip);
    expected.extend_from_slice(&[0xf8, 0xa7]);
    assert_eq!(event.encode(), expected);
    assert_eq!(latest::JamPeerTransportEvent::decode(&mut &expected[..]), Ok(event));
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportSendChunkRequest::V1(
            latest::HostJamPeerTransportSendChunkRequest {
                stream: 7, total: 3, offset: 1, bytes: vec![0xaa, 0xbb], fin: true,
            },
        ).encode(),
        vec![0, 7, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 8, 0xaa, 0xbb, 1],
    );
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportSendChunkResponse::V1.encode(), vec![0],
    );
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportRecvChunkRequest::V1(
            latest::HostJamPeerTransportRecvChunkRequest { stream: 7, max_bytes: 262144 },
        ).encode(),
        vec![0, 7, 0, 0, 0, 0, 0, 4, 0],
    );
    assert_eq!(
        jam_peer_transport::HostJamPeerTransportRecvChunkResponse::V1(
            latest::HostJamPeerTransportRecvChunkResponse {
                total: Some(3), offset: 1, bytes: vec![0xaa, 0xbb],
                complete: true, fin: false, reset: false,
            },
        ).encode(),
        vec![0, 1, 3, 0, 0, 0, 1, 0, 0, 0, 8, 0xaa, 0xbb, 1, 0, 0],
    );
    for (error, index) in [
        (latest::HostJamPeerTransportListenError::NotGranted, 0),
        (latest::HostJamPeerTransportListenError::Refused, 1),
        (latest::HostJamPeerTransportListenError::Limit, 2),
        (latest::HostJamPeerTransportListenError::Unreachable, 3),
    ] {
        assert_eq!(jam_peer_transport::HostJamPeerTransportListenError::V1(error).encode(), vec![0, index]);
    }
    for (error, index) in [
        (latest::HostJamPeerTransportSendChunkError::Closed, 0),
        (latest::HostJamPeerTransportSendChunkError::TooLarge, 1),
        (latest::HostJamPeerTransportSendChunkError::Limit, 2),
        (latest::HostJamPeerTransportSendChunkError::InvalidChunk, 3),
    ] {
        assert_eq!(jam_peer_transport::HostJamPeerTransportSendChunkError::V1(error).encode(), vec![0, index]);
    }
    for (error, index) in [
        (latest::HostJamPeerTransportRecvChunkError::Closed, 0),
        (latest::HostJamPeerTransportRecvChunkError::InvalidMaxBytes, 1),
    ] {
        assert_eq!(jam_peer_transport::HostJamPeerTransportRecvChunkError::V1(error).encode(), vec![0, index]);
    }
    assert_eq!(latest::JAM_PEER_TRANSPORT_MAX_CHUNK_BYTES, 256 << 10);
    assert_eq!(latest::JAM_PEER_TRANSPORT_MAX_LOGICAL_MESSAGE_BYTES, 64 << 20);
}

#[test]
fn unsupported_hosts_refuse_streaming_methods() {
    struct UnsupportedHost;
    #[truapi::async_trait]
    impl truapi::api::JamPeerTransport for UnsupportedHost {}
    use truapi::api::JamPeerTransport;
    futures::executor::block_on(async {
        let cx = truapi::CallContext::default();
        assert_eq!(
            UnsupportedHost.send_chunk(&cx,
                jam_peer_transport::HostJamPeerTransportSendChunkRequest::V1(
                    latest::HostJamPeerTransportSendChunkRequest {
                        stream: 7, total: 0, offset: 0, bytes: vec![], fin: true,
                    },
                ),
            ).await,
            Err(truapi::CallError::Unsupported),
        );
        assert_eq!(
            UnsupportedHost.recv_chunk(&cx,
                jam_peer_transport::HostJamPeerTransportRecvChunkRequest::V1(
                    latest::HostJamPeerTransportRecvChunkRequest { stream: 7, max_bytes: 1 },
                ),
            ).await,
            Err(truapi::CallError::Unsupported),
        );
    });
}
