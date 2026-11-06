use super::{
	AliveUntilDropped, Arc, AtomicBool, Ordering, TUNNELS, ensure_tunnel, lock, rewrite_host,
	tunnel_address, tunnel_wss_bootnodes,
};

/// Reuse is what makes one listener serve every chain naming the same
/// upstream.
#[test]
fn a_live_tunnel_is_reused_rather_than_rebound() {
	let host = "reuse.test.invalid";
	let first = ensure_tunnel(host, 443).expect("bind a tunnel");
	let second = ensure_tunnel(host, 443).expect("reuse the tunnel");

	assert_eq!(first, second);
}

/// The producer half: whatever ends the accept loop must clear the flag, or
/// `ensure_tunnel` keeps handing out a port nothing is listening on. Drives
/// `AliveUntilDropped` itself rather than setting the flag by hand, so
/// removing the guard fails this.
#[test]
fn the_guard_clears_the_flag_when_the_loop_ends() {
	let alive = Arc::new(AtomicBool::new(true));
	let owned = Arc::clone(&alive);

	std::thread::spawn(move || {
		let _guard = AliveUntilDropped(owned);
	})
	.join()
	.expect("guard thread");

	assert!(
		!alive.load(Ordering::Acquire),
		"the accept loop ended but the tunnel still reads as alive"
	);
}

/// The case the flag exists for. Asserts the replacement is live rather than
/// that its port differs: a genuinely dead tunnel releases its port, which
/// the kernel may hand straight back, so port inequality is not the property.
#[test]
fn a_dead_tunnel_is_replaced() {
	let host = "dead.test.invalid";
	ensure_tunnel(host, 443).expect("bind a tunnel");
	mark_dead(host, 443);

	let replacement = ensure_tunnel(host, 443).expect("rebind after death");

	assert_eq!(Some(replacement), live_port(host, 443));
}

/// A tunnel already marked dead is never handed back.
#[test]
fn a_dead_tunnel_is_not_reused() {
	let host = "stale.test.invalid";
	let dead = ensure_tunnel(host, 443).expect("bind a tunnel");
	mark_dead(host, 443);

	assert_ne!(live_port(host, 443), Some(dead));
}

fn mark_dead(host: &str, port: u16) {
	let tunnels = TUNNELS.get().expect("tunnel map initialised");
	lock(tunnels)
		.get(&(host.to_owned(), port))
		.expect("the tunnel just created")
		.alive
		.store(false, Ordering::Release);
}

/// The cached port, or `None` when the entry is absent or marked dead.
fn live_port(host: &str, port: u16) -> Option<u16> {
	let tunnels = TUNNELS.get().expect("tunnel map initialised");
	let guard = lock(tunnels);
	let tunnel = guard.get(&(host.to_owned(), port))?;
	tunnel.alive.load(Ordering::Acquire).then_some(tunnel.local)
}

/// Distinct upstreams must not share a listener, or a chain would be relayed
/// to the wrong node.
#[test]
fn distinct_upstreams_get_distinct_tunnels() {
	let one = ensure_tunnel("one.test.invalid", 443).expect("bind one");
	let two = ensure_tunnel("two.test.invalid", 443).expect("bind two");

	assert_ne!(one, two);
}

/// The same host on two ports is two upstreams.
#[test]
fn the_upstream_port_is_part_of_the_tunnel_identity() {
	let host = "ports.test.invalid";
	let https = ensure_tunnel(host, 443).expect("bind 443");
	let alt = ensure_tunnel(host, 9944).expect("bind 9944");

	assert_ne!(https, alt);
}

#[test]
fn a_plain_ws_bootnode_is_left_alone() {
	assert!(tunnel_address("/dns4/example.com/tcp/30333/ws/p2p/id").is_none());
}

#[test]
fn a_raw_tcp_bootnode_is_left_alone() {
	assert!(tunnel_address("/ip4/1.2.3.4/tcp/30333/p2p/id").is_none());
}

/// Relaying the transport is only safe for an address the noise handshake
/// can authenticate, so an address without a peer id must not be rewritten.
#[test]
fn a_wss_bootnode_without_a_peer_id_is_left_alone() {
	assert!(tunnel_address("/dns4/example.com/tcp/443/wss").is_none());
}

#[test]
fn a_wss_bootnode_keeps_its_peer_id_and_becomes_loopback_ws() {
	let rewritten = tunnel_address("/dns4/example.com/tcp/443/wss/p2p/12D3KooWabc")
		.expect("a dns wss bootnode is tunnelled");
	assert!(rewritten.starts_with("/ip4/127.0.0.1/tcp/"));
	assert!(rewritten.ends_with("/ws/p2p/12D3KooWabc"));
}

#[test]
fn the_tls_ws_spelling_is_tunnelled_too() {
	let rewritten = tunnel_address("/dns/example.com/tcp/443/tls/ws/p2p/12D3KooWabc")
		.expect("the /tls/ws spelling is tunnelled");
	assert!(rewritten.ends_with("/ws/p2p/12D3KooWabc"));
}

/// The upgrade smoldot writes names the loopback address it dialled, which a
/// virtual-hosted bootnode answers with 404. The relay has to put the real
/// authority back.
#[test]
fn the_host_header_is_rewritten_to_the_upstream_authority() {
	let upgrade = b"GET / HTTP/1.1\r\nHost: 127.0.0.1:54321\r\nUpgrade: websocket\r\n\r\n";
	let rewritten = rewrite_host(upgrade, "example.com", 443);
	let rewritten = String::from_utf8(rewritten).expect("headers stay utf-8");
	assert!(rewritten.contains("Host: example.com:443\r\n"), "{rewritten}");
	assert!(!rewritten.contains("127.0.0.1"), "{rewritten}");
	assert!(rewritten.contains("Upgrade: websocket\r\n"), "{rewritten}");
}

#[test]
fn a_lowercase_host_header_is_rewritten_too() {
	let upgrade = b"GET / HTTP/1.1\r\nhost: 127.0.0.1:1\r\n\r\n";
	let rewritten =
		String::from_utf8(rewrite_host(upgrade, "example.com", 443)).expect("headers stay utf-8");
	assert!(rewritten.contains("Host: example.com:443\r\n"), "{rewritten}");
}

#[test]
fn a_spec_without_bootnodes_is_returned_unchanged() {
	let spec = r#"{"name":"x","genesis":{"stateRootHash":"0x00"}}"#;
	assert_eq!(tunnel_wss_bootnodes(spec), spec);
}

#[test]
fn a_spec_whose_bootnodes_are_all_plain_is_returned_unchanged() {
	let spec = r#"{"bootNodes":["/ip4/1.2.3.4/tcp/30333/p2p/id"]}"#;
	assert_eq!(tunnel_wss_bootnodes(spec), spec);
}

/// Everything outside `bootNodes` must survive byte for byte, including
/// integers too wide for `f64`, which a full `Value` round trip would
/// silently change.
#[test]
fn a_wide_integer_elsewhere_in_the_spec_survives() {
	let spec = r#"{"bootNodes":["/dns4/example.com/tcp/443/wss/p2p/id"],"big":123456789012345678901234567890}"#;
	let rewritten = tunnel_wss_bootnodes(spec);
	assert!(rewritten.contains("123456789012345678901234567890"), "{rewritten}");
	assert!(rewritten.contains("/ip4/127.0.0.1/tcp/"), "{rewritten}");
}
