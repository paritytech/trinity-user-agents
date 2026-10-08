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
    let mut session = JamPeerSession::new();
    session.dial_deadline = dial_deadline;
    session
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
    peer_with_stream_credit(identity, reply, 100)
}

fn peer_with_stream_credit(
    identity: &Identity,
    reply: &'static [u8],
    credit: u32,
) -> (quinn::Endpoint, u16) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(identity.cert_chain(), identity.private_key())
        .unwrap();
    tls.alpn_protocols = vec![super::super::alpn(&GENESIS).into_bytes()];
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap();
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    let mut transport = quinn::TransportConfig::default();
    transport.max_concurrent_bidi_streams(credit.into());
    config.transport_config(Arc::new(transport));
    let endpoint = quinn::Endpoint::server(
        config,
        (Ipv4Addr::LOCALHOST, 0).into(),
    )
    .unwrap();
    let port = endpoint.local_addr().unwrap().port();
    let server = endpoint.clone();
    tokio::spawn(async move {
        let connection = server.accept().await.unwrap().await.unwrap();
        // Admission/lifecycle tests intentionally close without opening a stream.
        let Ok((mut send, mut recv)) = connection.accept_bi().await else {
            return;
        };
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
            .open(&cx(), wire::HostJamPeerTransportOpenRequest::V1(
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
    assert_eq!(session.pending_dials.load(Ordering::Acquire), 0);
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

#[tokio::test]
async fn revoking_a_session_interrupts_a_pending_permission_prompt() {
    let session = JamPeerSession::new();
    let (_silent, port) = silent_port();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (held, dropped) = tokio::sync::oneshot::channel::<()>();
    let call_context = cx();
    let spawner = test_spawner();
    let mut dial = Box::pin(session.dial(
        &call_context,
        dial_request(GENESIS, port, [7; 32]),
        || async move {
            let _held = held;
            let _ = started.send(());
            std::future::pending::<Decision>().await
        },
        &spawner,
    ));
    tokio::select! {
        _ = &mut dial => panic!("an unanswered prompt completed"),
        _ = ready => {}
    }
    session.revoke();
    let result = tokio::time::timeout(Duration::from_millis(100), dial)
        .await
        .expect("revocation must not wait for the dial deadline");
    assert!(matches!(result, Err(CallError::Denied)));
    assert!(session.existing().is_none());
    assert!(tokio::time::timeout(Duration::from_secs(1), dropped).await.unwrap().is_err());
}

#[tokio::test]
async fn a_pre_cancelled_dial_does_not_prompt_or_bind() {
    let session = JamPeerSession::new();
    let call_context = cx();
    call_context.cancel().cancel();
    let result = session.dial(
        &call_context,
        dial_request(GENESIS, 1, [7; 32]),
        || async { panic!("a cancelled dial must not start authorization") },
        &test_spawner(),
    ).await;
    assert!(matches!(result, Err(CallError::Cancelled)));
    assert!(session.existing().is_none());
}

#[tokio::test]
async fn cancelling_open_does_not_wait_for_the_peers_stream_credit() {
    let identity = Identity::generate().unwrap();
    let (_peer, port) = peer_with_stream_credit(&identity, b"unused", 0);
    let session = JamPeerSession::new();
    let wire::HostJamPeerTransportDialResponse::V1(response) = session.dial(
        &cx(),
        dial_request(GENESIS, port, *identity.public()),
        || async { Ok(()) },
        &test_spawner(),
    ).await.unwrap();
    let call_context = cx();
    let request = wire::HostJamPeerTransportOpenRequest::V1(
        latest::HostJamPeerTransportOpenRequest { conn: response.conn, kind: 0 },
    );
    let mut open = Box::pin(session.open(&call_context, request));
    assert!(futures::poll!(&mut open).is_pending());
    call_context.cancel().cancel();
    let result = tokio::time::timeout(Duration::from_millis(100), open).await.unwrap();
    assert!(matches!(result, Err(CallError::Cancelled)));
}

#[tokio::test]
async fn permission_waits_are_bounded_before_prompting_and_cancelled_slots_are_reusable() {
    let session = JamPeerSession::new();
    let asked = Arc::new(AtomicUsize::new(0));
    let (_answer, decision) = watch::channel(false);
    let spawner = test_spawner();
    let contexts: Vec<_> = (0..quic::MAX_CONNECTIONS).map(|_| cx()).collect();
    let mut dials: Vec<_> = contexts.iter().enumerate().map(|(index, context)| {
        Box::pin(session.dial(
            context,
            dial_request([index as u8; 32], 1, [7; 32]),
            || prompt(&asked, &decision),
            &spawner,
        ))
    }).collect();
    for dial in &mut dials {
        assert!(futures::poll!(dial.as_mut()).is_pending());
    }
    assert_eq!(asked.load(Ordering::SeqCst), quic::MAX_CONNECTIONS);
    assert_eq!(session.pending_dials.load(Ordering::Acquire), quic::MAX_CONNECTIONS);
    assert!(session.existing().is_none());

    let ninth = session.dial(
        &cx(),
        dial_request([0; 32], 1, [7; 32]),
        || async { panic!("capacity must be checked before authorization") },
        &spawner,
    ).await.unwrap_err();
    assert_eq!(dial_failure(ninth), Some(latest::HostJamPeerTransportDialError::Limit));
    contexts[0].cancel().cancel();
    assert!(matches!(dials[0].as_mut().await, Err(CallError::Cancelled)));
    assert_eq!(session.pending_dials.load(Ordering::Acquire), quic::MAX_CONNECTIONS - 1);

    let retry_context = cx();
    let mut retry = Box::pin(session.dial(
        &retry_context,
        dial_request([0; 32], 1, [7; 32]),
        || async { panic!("a cached pending decision must not prompt again") },
        &spawner,
    ));
    assert!(futures::poll!(&mut retry).is_pending());
    assert_eq!(session.pending_dials.load(Ordering::Acquire), quic::MAX_CONNECTIONS);
    drop(retry);
    drop(dials);
    assert_eq!(session.pending_dials.load(Ordering::Acquire), 0);

    // Pending decisions remain bounded even after every caller has left.
    let ninth = session.dial(
        &cx(),
        dial_request([9; 32], 1, [7; 32]),
        || async { panic!("the decision cache must not evict a pending check") },
        &spawner,
    ).await.unwrap_err();
    assert_eq!(dial_failure(ninth), Some(latest::HostJamPeerTransportDialError::Limit));
    assert_eq!(session.pending_dials.load(Ordering::Acquire), 0);
    assert_eq!(session.decisions.lock().len(), quic::MAX_CONNECTIONS);
    session.revoke();
}

#[tokio::test]
async fn denied_genesis_decisions_are_retained_and_the_ninth_is_limited() {
    let session = JamPeerSession::new();
    let asked = AtomicUsize::new(0);
    let spawner = test_spawner();
    for index in 0..quic::MAX_CONNECTIONS {
        let result = session.dial(
            &cx(),
            dial_request([index as u8; 32], 1, [7; 32]),
            || {
                asked.fetch_add(1, Ordering::SeqCst);
                async { not_granted() }
            },
            &spawner,
        ).await.unwrap_err();
        assert_eq!(dial_failure(result), Some(latest::HostJamPeerTransportDialError::NotGranted));
    }
    for (genesis, expected) in [
        ([9; 32], latest::HostJamPeerTransportDialError::Limit),
        ([0; 32], latest::HostJamPeerTransportDialError::NotGranted),
    ] {
        let result = session.dial(
            &cx(),
            dial_request(genesis, 1, [7; 32]),
            || async { panic!("denied decisions must not be evicted or re-prompted") },
            &spawner,
        ).await.unwrap_err();
        assert_eq!(dial_failure(result), Some(expected));
    }
    assert_eq!(asked.load(Ordering::SeqCst), quic::MAX_CONNECTIONS);
    assert_eq!(session.decisions.lock().len(), quic::MAX_CONNECTIONS);
    assert_eq!(session.pending_dials.load(Ordering::Acquire), 0);
    assert!(session.existing().is_none());
}

#[tokio::test]
async fn live_connections_and_permission_waits_share_capacity_and_close_releases_it() {
    let session = JamPeerSession::new();
    let identity = Identity::generate().unwrap();
    let spawner = test_spawner();
    let mut peers = Vec::new();
    let mut conns = Vec::new();
    for _ in 0..quic::MAX_CONNECTIONS {
        let (peer, port) = peer(&identity, b"unused");
        peers.push(peer);
        let wire::HostJamPeerTransportDialResponse::V1(response) = session.dial(
            &cx(),
            dial_request(GENESIS, port, *identity.public()),
            || async { Ok(()) },
            &spawner,
        ).await.unwrap();
        conns.push(response.conn);
    }
    assert_eq!(session.pending_dials.load(Ordering::Acquire), 0);
    let full = session.dial(
        &cx(),
        dial_request([9; 32], 1, [7; 32]),
        || async { panic!("live handles must exclude new permission waits") },
        &spawner,
    ).await.unwrap_err();
    assert_eq!(dial_failure(full), Some(latest::HostJamPeerTransportDialError::Limit));
    assert_eq!(session.decisions.lock().len(), 1);

    session.close(wire::HostJamPeerTransportCloseRequest::V1(
        latest::HostJamPeerTransportCloseRequest { conn: conns[0] },
    )).unwrap();
    let (_answer, decision) = watch::channel(false);
    let asked = Arc::new(AtomicUsize::new(0));
    let context = cx();
    let mut pending = Box::pin(session.dial(
        &context,
        dial_request([9; 32], 1, [7; 32]),
        || prompt(&asked, &decision),
        &spawner,
    ));
    assert!(futures::poll!(&mut pending).is_pending());
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(session.pending_dials.load(Ordering::Acquire), 1);
    let full = session.dial(
        &cx(),
        dial_request([10; 32], 1, [7; 32]),
        || async { panic!("live plus pending must share the eight slots") },
        &spawner,
    ).await.unwrap_err();
    assert_eq!(dial_failure(full), Some(latest::HostJamPeerTransportDialError::Limit));
    session.revoke();
    assert!(matches!(pending.await, Err(CallError::Denied)));
    assert_eq!(session.pending_dials.load(Ordering::Acquire), 0);
    drop(peers);
}
