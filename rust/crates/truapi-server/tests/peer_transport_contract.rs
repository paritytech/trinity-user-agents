//! PeerTransport contract regression test.
//!
//! Pins the frozen trait-23 wire ids and SCALE layouts, the `JamPeers`
//! permission's place in `RemotePermission`, and the genesis/ALPN helpers a
//! host uses on dial.

use parity_scale_codec::{Decode, Encode};
use truapi::latest;
use truapi::versioned::peer_transport;
use truapi_server::generated::wire_table::{
    MethodIds, PEER_TRANSPORT_CLOSE, PEER_TRANSPORT_DIAL, PEER_TRANSPORT_EVENTS,
    PEER_TRANSPORT_OPEN, PEER_TRANSPORT_RECV, PEER_TRANSPORT_RESET, PEER_TRANSPORT_SEND,
};
use truapi_server::peer_transport::{InvalidGenesis, alpn, parse_genesis};

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

/// The frozen contract: namespace 23, methods 0..6 in this order, V1 payloads.
#[test]
fn the_wire_ids_and_scale_layout_match_the_frozen_contract() {
    for (ids, method_id) in [
        (PEER_TRANSPORT_DIAL, 0),
        (PEER_TRANSPORT_OPEN, 1),
        (PEER_TRANSPORT_SEND, 2),
        (PEER_TRANSPORT_RECV, 3),
        (PEER_TRANSPORT_RESET, 4),
        (PEER_TRANSPORT_CLOSE, 5),
        (PEER_TRANSPORT_EVENTS, 6),
    ] {
        assert_eq!(
            ids,
            MethodIds {
                trait_id: 23,
                method_id
            }
        );
    }

    let dial =
        peer_transport::HostPeerTransportDialRequest::V1(latest::HostPeerTransportDialRequest {
            genesis: genesis(),
            ip: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 127, 0, 0, 1],
            port: 43000,
            ed25519: [0x11; 32],
            p256: Some([0x02; 33]),
        })
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
        peer_transport::HostPeerTransportSendRequest::V1(latest::HostPeerTransportSendRequest {
            stream: 7,
            message: vec![0xaa, 0xbb],
            fin: true,
        })
        .encode(),
        vec![0, 7, 0, 0, 0, 8, 0xaa, 0xbb, 1]
    );
    assert_eq!(
        peer_transport::HostPeerTransportRecvResponse::V1(latest::HostPeerTransportRecvResponse {
            message: None,
            fin: false,
            reset: true,
        })
        .encode(),
        vec![0, 0, 0, 1]
    );
    assert_eq!(
        peer_transport::HostPeerTransportEventsResponse::V1(
            latest::HostPeerTransportEventsResponse {
                events: vec![
                    latest::PeerTransportEvent::ConnClosed { conn: 1 },
                    latest::PeerTransportEvent::StreamFin { stream: 2 },
                    latest::PeerTransportEvent::Accepted {
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
        peer_transport::HostPeerTransportDialError::V1(
            latest::HostPeerTransportDialError::Unreachable
        )
        .encode(),
        vec![0, 3]
    );
    assert_eq!(
        peer_transport::HostPeerTransportEventsRequest::V1.encode(),
        vec![0]
    );
    assert_eq!(latest::PEER_TRANSPORT_MAX_CONNECTIONS, 8);
    assert_eq!(latest::PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION, 16);
    assert_eq!(latest::PEER_TRANSPORT_MAX_MESSAGE_BYTES, 1 << 20);
    assert_eq!(
        latest::PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION,
        4 << 20
    );
}
