use std::net::Ipv4Addr;
use std::sync::atomic::AtomicUsize;

use super::super::tls::Identity;
use super::*;
use crate::test_support::test_spawner;

const GENESIS: [u8; 32] = [0x35; 32];

fn cx() -> CallContext {
    CallContext::with_request_id("t".into())
}

fn not_granted() -> Decision {
    Err(dial_error(
        latest::HostJamPeerTransportDialError::NotGranted,
    ))
}

/// What the runtime's permission check answers: `GENESIS` only, counting how
/// often it is asked.
fn grant_genesis(
    asked: &Arc<AtomicUsize>,
    genesis: [u8; 32],
) -> impl Future<Output = Decision> + Send + 'static {
    asked.fetch_add(1, Ordering::SeqCst);
    async move {
        if genesis == GENESIS {
            Ok(())
        } else {
            not_granted()
        }
    }
}

/// A permission check that answers once `decision` fires.
fn prompt(
    asked: &Arc<AtomicUsize>,
    decision: &watch::Receiver<bool>,
) -> impl Future<Output = Decision> + Send + 'static {
    asked.fetch_add(1, Ordering::SeqCst);
    let mut decision = decision.clone();
    async move {
        let _ = decision.changed().await;
        let granted = *decision.borrow();
        if granted { Ok(()) } else { not_granted() }
    }
}

fn session_with_deadline(dial_deadline: Duration) -> JamPeerSession {
    JamPeerSession {
        dial_deadline,
        ..JamPeerSession::new()
    }
}

fn dial_request(
    genesis: [u8; 32],
    port: u16,
    ed25519: [u8; 32],
) -> wire::HostJamPeerTransportDialRequest {
    wire::HostJamPeerTransportDialRequest::V1(latest::HostJamPeerTransportDialRequest {
        genesis,
        ip: Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets(),
        port,
        ed25519,
        p256: None,
    })
}

fn domain<E>(error: CallError<E>) -> Option<E> {
    match error {
        CallError::Domain(error) => Some(error),
        _ => None,
    }
}

fn dial_failure(
    error: CallError<wire::HostJamPeerTransportDialError>,
) -> Option<latest::HostJamPeerTransportDialError> {
    domain(error).map(|wire::HostJamPeerTransportDialError::V1(error)| error)
}

/// A bound socket that never answers keeps every dial in its handshake.
fn silent_port() -> (std::net::UdpSocket, u16) {
    let silent = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = silent.local_addr().unwrap().port();
    (silent, port)
}

/// A JAMNP-S peer on loopback: presents a certificate for `identity`, then
/// answers the first message of the first stream with `reply` and finishes.
fn peer(identity: &Identity, reply: &'static [u8]) -> (quinn::Endpoint, u16) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(identity.cert_chain(), identity.private_key())
        .unwrap();
    tls.alpn_protocols = vec![super::super::alpn(&GENESIS).into_bytes()];
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap();
    let endpoint = quinn::Endpoint::server(
        quinn::ServerConfig::with_crypto(Arc::new(crypto)),
        (Ipv4Addr::LOCALHOST, 0).into(),
    )
    .unwrap();
    let port = endpoint.local_addr().unwrap().port();
    let server = endpoint.clone();
    tokio::spawn(async move {
        let connection = server.accept().await.unwrap().await.unwrap();
        let (mut send, mut recv) = connection.accept_bi().await.unwrap();
        let mut kind = [0u8; 1];
        recv.read_exact(&mut kind).await.unwrap();
        let mut length = [0u8; 4];
        recv.read_exact(&mut length).await.unwrap();
        let mut message = vec![0u8; u32::from_le_bytes(length) as usize];
        recv.read_exact(&mut message).await.unwrap();
        assert_eq!(kind, [0], "the host sends the stream kind first");
        assert_eq!(message, b"hello", "the host frames the message");
        send.write_all(&(reply.len() as u32).to_le_bytes())
            .await
            .unwrap();
        send.write_all(reply).await.unwrap();
        send.finish().unwrap();
        connection.closed().await;
    });
    (endpoint, port)
}

#[test]
fn a_granted_dial_frames_messages_outside_tokio_and_revoke_denies_everything() {
    // The native runtime drives host traits on a futures executor: no tokio
    // timer or reactor exists on the calling thread.
    let server = tokio::runtime::Runtime::new().unwrap();
    let identity = Identity::generate().unwrap();
    let (_peer, port) = {
        let _context = server.enter();
        peer(&identity, b"welcome")
    };
    let asked = Arc::new(AtomicUsize::new(0));
    let spawner = test_spawner();
    futures::executor::block_on(async {
        let session = JamPeerSession::new();
        let wire::HostJamPeerTransportDialResponse::V1(latest::HostJamPeerTransportDialResponse {
            conn,
        }) = session
            .dial(
                &cx(),
                dial_request(GENESIS, port, *identity.public()),
                || grant_genesis(&asked, GENESIS),
                &spawner,
            )
            .await
            .unwrap();
        let wire::HostJamPeerTransportOpenResponse::V1(latest::HostJamPeerTransportOpenResponse {
            stream,
        }) = session
            .open(wire::HostJamPeerTransportOpenRequest::V1(
                latest::HostJamPeerTransportOpenRequest { conn, kind: 0 },
            ))
            .await
            .unwrap();
        session
            .send(wire::HostJamPeerTransportSendRequest::V1(
                latest::HostJamPeerTransportSendRequest {
                    stream,
                    message: b"hello".to_vec(),
                    fin: false,
                },
            ))
            .unwrap();

        // Poll until the peer's reply and its finish have been reported. The
        // finish may ride on the last message or come alone after it.
        let recv =
            wire::HostJamPeerTransportRecvRequest::V1(latest::HostJamPeerTransportRecvRequest {
                stream,
                max: 1024,
            });
        let mut messages = Vec::new();
        let mut end = None;
        for _ in 0..500 {
            let wire::HostJamPeerTransportRecvResponse::V1(response) =
                session.recv(recv.clone()).unwrap();
            match response.message {
                Some(message) => messages.push(message),
                None if response.fin || response.reset => {
                    end = Some(response);
                    break;
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        assert_eq!(
            (messages, end),
            (
                vec![b"welcome".to_vec()],
                Some(latest::HostJamPeerTransportRecvResponse {
                    message: None,
                    fin: true,
                    reset: false,
                }),
            ),
            "the host strips the length, then reports the finish once drained",
        );
        assert_eq!(
            session.recv(recv.clone()).map_err(domain),
            Err(Some(wire::HostJamPeerTransportRecvError::V1(
                latest::HostJamPeerTransportRecvError::Closed
            ))),
            "a fully consumed receive side is closed",
        );

        session.revoke();
        let denied = [
            session
                .dial(
                    &cx(),
                    dial_request(GENESIS, port, *identity.public()),
                    || grant_genesis(&asked, GENESIS),
                    &spawner,
                )
                .await
                .is_err_and(|error| matches!(error, CallError::Denied)),
            session
                .recv(recv.clone())
                .is_err_and(|error| matches!(error, CallError::Denied)),
            session
                .events()
                .is_err_and(|error| matches!(error, CallError::Denied)),
        ];
        assert_eq!(denied, [true; 3], "a revoked session denies every call");
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_genesis_binds_nothing_and_a_foreign_key_is_refused() {
    let identity = Identity::generate().unwrap();
    let (_peer, port) = peer(&identity, b"unused");
    let asked = Arc::new(AtomicUsize::new(0));
    let spawner = test_spawner();
    let session = JamPeerSession::new();

    let foreign = session
        .dial(
            &cx(),
            dial_request([0x11; 32], port, *identity.public()),
            || grant_genesis(&asked, [0x11; 32]),
            &spawner,
        )
        .await
        .unwrap_err();
    assert_eq!(
        (dial_failure(foreign), session.existing().is_none()),
        (
            Some(latest::HostJamPeerTransportDialError::NotGranted),
            true
        ),
        "a refused dial never creates the QUIC endpoint, so no packet leaves",
    );

    let impostor = Identity::generate().unwrap();
    let refused = session
        .dial(
            &cx(),
            dial_request(GENESIS, port, *impostor.public()),
            || grant_genesis(&asked, GENESIS),
            &spawner,
        )
        .await
        .unwrap_err();
    assert_eq!(
        dial_failure(refused),
        Some(latest::HostJamPeerTransportDialError::Refused),
        "a certificate for another key is refused"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_dials_never_exceed_the_connection_cap_and_ask_once() {
    let asked = Arc::new(AtomicUsize::new(0));
    let session = Arc::new(JamPeerSession::new());
    let (silent, port) = silent_port();
    let dials = (0..quic::MAX_CONNECTIONS + 4).map(|_| {
        let session = session.clone();
        let asked = asked.clone();
        tokio::spawn(async move {
            session
                .dial(
                    &cx(),
                    dial_request(GENESIS, port, [7; 32]),
                    || grant_genesis(&asked, GENESIS),
                    &test_spawner(),
                )
                .await
                .unwrap_err()
        })
    });
    let (mut unreachable, mut limited) = (0, 0);
    for dial in dials.collect::<Vec<_>>() {
        match dial_failure(dial.await.unwrap()) {
            Some(latest::HostJamPeerTransportDialError::Unreachable) => unreachable += 1,
            Some(latest::HostJamPeerTransportDialError::Limit) => limited += 1,
            other => panic!("unexpected dial result {other:?}"),
        }
    }
    assert_eq!(
        (unreachable, limited),
        (quic::MAX_CONNECTIONS, 4),
        "at most the cap dials at once; every dial beyond it is refused immediately",
    );
    // Released slots are reusable.
    let again = session
        .dial(
            &cx(),
            dial_request(GENESIS, port, [7; 32]),
            || grant_genesis(&asked, GENESIS),
            &test_spawner(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        (dial_failure(again), asked.load(Ordering::SeqCst)),
        (Some(latest::HostJamPeerTransportDialError::Unreachable), 1),
        "concurrent dials of one genesis ask once",
    );
    drop(silent);
    // Dropping the session here, inside a runtime worker, must not panic.
}

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_outlasting_the_deadline_is_unreachable_and_remembered() {
    let identity = Identity::generate().unwrap();
    let (_peer, port) = peer(&identity, b"unused");
    let asked = Arc::new(AtomicUsize::new(0));
    let (answer, decision) = watch::channel(false);
    let spawner = test_spawner();
    let session = session_with_deadline(Duration::from_millis(50));

    let late = session
        .dial(
            &cx(),
            dial_request(GENESIS, port, *identity.public()),
            || prompt(&asked, &decision),
            &spawner,
        )
        .await
        .unwrap_err();
    assert_eq!(
        dial_failure(late),
        Some(latest::HostJamPeerTransportDialError::Unreachable)
    );
    // The user answers after the guest stopped waiting: nothing was opened.
    answer.send(true).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(session.existing().is_none());

    session
        .dial(
            &cx(),
            dial_request(GENESIS, port, *identity.public()),
            || prompt(&asked, &decision),
            &spawner,
        )
        .await
        .expect("the retry reuses the remembered grant");
    assert_eq!(
        asked.load(Ordering::SeqCst),
        1,
        "the retry does not ask again"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_during_the_prompt_is_cancelled_and_opens_nothing() {
    let asked = Arc::new(AtomicUsize::new(0));
    let (answer, decision) = watch::channel(false);
    let spawner = test_spawner();
    let session = JamPeerSession::new();
    let cx = cx();
    let cancel = cx.cancel().clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();
    });
    let (_silent, port) = silent_port();

    let error = session
        .dial(
            &cx,
            dial_request(GENESIS, port, [7; 32]),
            || prompt(&asked, &decision),
            &spawner,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CallError::Cancelled));
    answer.send(true).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Every slot is free: the cancelled dial left no handshake behind.
    let dials = (0..quic::MAX_CONNECTIONS).map(|_| {
        session.dial_granted(
            latest::HostJamPeerTransportDialRequest {
                genesis: GENESIS,
                ip: Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets(),
                port,
                ed25519: [7; 32],
                p256: None,
            },
            || prompt(&asked, &decision),
            &spawner,
        )
    });
    for result in futures::future::join_all(dials).await {
        assert_eq!(
            dial_failure(result.unwrap_err()),
            Some(latest::HostJamPeerTransportDialError::Unreachable)
        );
    }
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handshake_outlasting_the_deadline_frees_its_slot() {
    let asked = Arc::new(AtomicUsize::new(0));
    let spawner = test_spawner();
    let session = session_with_deadline(Duration::from_millis(50));
    let (_silent, port) = silent_port();
    for _ in 0..quic::MAX_CONNECTIONS + 2 {
        let error = session
            .dial(
                &cx(),
                dial_request(GENESIS, port, [7; 32]),
                || grant_genesis(&asked, GENESIS),
                &spawner,
            )
            .await
            .unwrap_err();
        assert_eq!(
            dial_failure(error),
            Some(latest::HostJamPeerTransportDialError::Unreachable),
            "an expired dial never holds a slot, so none hits Limit"
        );
    }
}
