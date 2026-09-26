use parity_scale_codec::Encode;
use truapi::latest;
use truapi::versioned::peer_transport;
use truapi_server::generated::wire_table::{
    MethodIds, PEER_TRANSPORT_CLOSE, PEER_TRANSPORT_DIAL, PEER_TRANSPORT_EVENTS,
    PEER_TRANSPORT_OPEN, PEER_TRANSPORT_RECV, PEER_TRANSPORT_RESET, PEER_TRANSPORT_SEND,
};
use truapi_server::peer_transport::{PeerTransportGrant, PeerTransportGrantError, alpn};

const GENESIS_HEX: &str = "353963b9cedfe4ea22038081052a5c151b06b55a4a026a97522cd0320cabf49f";

fn genesis() -> [u8; 32] {
    truapi_server::peer_transport::parse_genesis(GENESIS_HEX).unwrap()
}

fn manifest(capabilities: &str) -> Vec<u8> {
    format!(
        r#"{{"$v":2,"kind":"app","appVersion":[0,1,0],"runtime":{{"kind":"polkavm","abiVersion":1,"entrypoint":"app.polkavm"}},"capabilities":{capabilities}}}"#
    )
    .into_bytes()
}

#[test]
fn the_manifest_capability_grants_exactly_its_genesis() {
    let grant = PeerTransportGrant::from_manifest(&manifest(&format!(
        r#"{{"graphics":{{"abiVersion":1,"profile":"framebuffer"}},"network":{{"jam":{{"genesis":"{GENESIS_HEX}"}}}}}}"#
    )))
    .unwrap()
    .expect("capability present");
    assert_eq!(grant.genesis, genesis());
    assert!(grant.permits(&genesis()));
    assert!(!grant.permits(&[0; 32]));
    assert_eq!(grant.alpn(), "jamnp-s/1/353963b9");
    assert_eq!(alpn(&[0xab; 32]), "jamnp-s/1/abababab");
    assert_eq!(grant.to_string(), format!("jam:{GENESIS_HEX}"));

    let prefixed = PeerTransportGrant::from_manifest(&manifest(&format!(
        r#"{{"network":{{"jam":{{"genesis":"0x{GENESIS_HEX}"}}}}}}"#
    )))
    .unwrap();
    assert_eq!(prefixed, Some(grant));
}

#[test]
fn a_manifest_without_the_capability_grants_nothing() {
    for capabilities in [
        r#"{"graphics":{"abiVersion":1,"profile":"framebuffer"}}"#,
        r#"{"network":{}}"#,
        r#"{"network":{"http":{"origins":["https://example.invalid"]}}}"#,
    ] {
        assert_eq!(
            PeerTransportGrant::from_manifest(&manifest(capabilities)).unwrap(),
            None,
            "{capabilities}"
        );
    }
}

#[test]
fn a_malformed_capability_is_refused_rather_than_ignored() {
    for (capabilities, error) in [
        (
            r#"{"network":{"jam":true}}"#.to_string(),
            PeerTransportGrantError::InvalidCapability,
        ),
        (
            r#"{"network":{"jam":{}}}"#.to_string(),
            PeerTransportGrantError::InvalidCapability,
        ),
        (
            r#"{"network":{"jam":{"genesis":7}}}"#.to_string(),
            PeerTransportGrantError::InvalidGenesis,
        ),
        (
            format!(
                r#"{{"network":{{"jam":{{"genesis":"{}"}}}}}}"#,
                &GENESIS_HEX[..62]
            ),
            PeerTransportGrantError::InvalidGenesis,
        ),
        (
            format!(
                r#"{{"network":{{"jam":{{"genesis":"{}"}}}}}}"#,
                GENESIS_HEX.to_uppercase()
            ),
            PeerTransportGrantError::InvalidGenesis,
        ),
        (
            format!(r#"{{"network":{{"jam":{{"genesis":"{GENESIS_HEX}0"}}}}}}"#),
            PeerTransportGrantError::InvalidGenesis,
        ),
    ] {
        assert_eq!(
            PeerTransportGrant::from_manifest(&manifest(&capabilities)),
            Err(error),
            "{capabilities}"
        );
    }
    assert_eq!(
        PeerTransportGrant::from_manifest(br#"{"$v":1,"trustedProducts":{}}"#),
        Err(PeerTransportGrantError::InvalidManifest)
    );
    assert_eq!(
        PeerTransportGrant::from_manifest(b"[]"),
        Err(PeerTransportGrantError::InvalidManifest)
    );
    assert_eq!(
        PeerTransportGrant::from_manifest(b"{"),
        Err(PeerTransportGrantError::InvalidManifest)
    );
}

/// The frozen contract: namespace 21, methods 0..6 in this order, V1 payloads.
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
                trait_id: 21,
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
