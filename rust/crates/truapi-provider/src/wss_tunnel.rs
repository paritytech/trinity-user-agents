//! Loopback TLS tunnels that make `/wss` bootnodes reachable on native targets.
//!
//! smoldot's native platform declines secure WebSocket:
//! `DefaultPlatform::supports_connection_type` omits
//! `ConnectionType::WebSocketDns { secure: true }` above a `TODO: support
//! WebSocket secure`, and `connect_stream` only matches `secure: false`. A chain
//! whose bootnodes are all `/wss` therefore never dials one — the network logs
//! `reason=no-address`, discovery reports `discovery-skipped-no-peer`, and warp
//! sync sits on the chain-spec checkpoint forever, so every read times out.
//!
//! Browsers are unaffected, which is why the same chain specs work in a page:
//! the wasm platform delegates to the browser's own `WebSocket`, and that speaks
//! TLS. Only the native platform has to terminate TLS itself.
//!
//! [`tunnel_wss_bootnodes`] rewrites each `/wss` bootnode to a `/ws` address on
//! a loopback listener that relays to the real host over TLS.
//!
//! The relay is not byte-for-byte. smoldot derives the WebSocket `Host` header
//! from the address it dials, so a rewritten address would send
//! `Host: 127.0.0.1:<port>` upstream, and a bootnode behind name-based virtual
//! hosting answers that with 404 or 403 rather than an upgrade. The relay
//! therefore rewrites that one header back to the real authority before
//! forwarding, and copies every later byte untouched.
//!
//! Only the transport is terminated: the libp2p noise handshake travels inside
//! the WebSocket frames, so the remote peer id is still authenticated end to end
//! and a tampering tunnel would fail the handshake rather than go unnoticed.
//! That argument only holds for an address that carries a peer id, so
//! [`tunnel_address`] declines any bootnode without one.
//!
//! Remove this module once smoldot's native platform supports secure WebSocket;
//! the only caller is the `add_chain` path in [`crate::light`].

use core::time::Duration;
use std::{
	collections::HashMap,
	sync::{
		Arc, Mutex, MutexGuard, OnceLock, PoisonError,
		atomic::{AtomicBool, Ordering},
	},
};

use serde_json::value::RawValue;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional},
	net::{TcpListener, TcpStream},
};
use tokio_rustls::{
	TlsConnector,
	rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};

/// Upstream `(host, port)` to the loopback tunnel relaying to it. One tunnel per
/// upstream, reused by every chain that names it, so the chains of a network
/// that share a hostname share a single listener.
static TUNNELS: OnceLock<Mutex<HashMap<(String, u16), Tunnel>>> = OnceLock::new();

/// A running tunnel: the loopback port it listens on, and whether its accept
/// loop is still running.
struct Tunnel {
	/// Loopback port the accept loop is bound to.
	local: u16,
	/// Cleared when the accept loop ends, so a dead port is never reused.
	alive: Arc<AtomicBool>,
}

/// Clears a tunnel's `alive` flag when its accept loop ends, so a dead port is
/// never handed out as live. A guard rather than a store at each exit because
/// the loop has four of them. It also covers an unwind, but the release profile
/// sets `panic = "abort"`, so that is a property of test builds only.
struct AliveUntilDropped(Arc<AtomicBool>);

impl Drop for AliveUntilDropped {
	fn drop(&mut self) {
		self.0.store(false, Ordering::Release);
	}
}

/// How long a relayed connection may take to reach the upstream and finish its
/// TLS handshake. The relay itself is not deadlined: a tunnelled connection
/// lives as long as the chain that opened it, and ends when smoldot closes its
/// side.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(20);

/// Cap on the WebSocket upgrade request the relay buffers before rewriting its
/// `Host` header. smoldot's is a few hundred bytes; anything past this is not a
/// handshake this tunnel should be repairing.
const MAX_UPGRADE_BYTES: usize = 8 * 1024;

/// Lock the tunnel map, recovering the guard if a previous holder panicked.
///
/// Mirrors [`crate::light`]: one poisoning event must not disable tunnelling for
/// the rest of the process, which would leave every `/wss` chain undialable with
/// no way back.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Rewrite every `/wss` bootnode in `spec` to a loopback `/ws` tunnel.
///
/// Returns `spec` unchanged when it carries no `/wss` bootnode, when it is not
/// the JSON object shape a chain spec has, or when a tunnel cannot be started —
/// a failure here must degrade to the previous behaviour rather than prevent the
/// chain from being added at all.
///
/// Only the `bootNodes` member is re-encoded. The rest of the document is
/// carried as raw JSON so that a spec written with integers wider than `u64`
/// survives byte for byte; parsing the whole document into `serde_json::Value`
/// would round those through `f64` and change the genesis it describes.
pub fn tunnel_wss_bootnodes(spec: &str) -> String {
	// Cheap reject before the parse: most specs, and every wasm build, have no
	// `/wss` bootnode at all.
	if !spec.contains("/wss") && !spec.contains("/tls/ws") {
		return spec.to_owned();
	}

	let Ok(mut parsed) = serde_json::from_str::<HashMap<String, Box<RawValue>>>(spec) else {
		return spec.to_owned();
	};
	let Some(raw_boot_nodes) = parsed.get("bootNodes") else {
		return spec.to_owned();
	};
	let Ok(mut boot_nodes) = serde_json::from_str::<Vec<String>>(raw_boot_nodes.get()) else {
		return spec.to_owned();
	};

	let mut rewrote = false;
	for entry in boot_nodes.iter_mut() {
		let Some(tunnelled) = tunnel_address(entry) else {
			continue;
		};
		*entry = tunnelled;
		rewrote = true;
	}

	if !rewrote {
		return spec.to_owned();
	}
	let Ok(encoded) = serde_json::to_string(&boot_nodes) else {
		return spec.to_owned();
	};
	let Ok(raw) = RawValue::from_string(encoded) else {
		return spec.to_owned();
	};
	parsed.insert("bootNodes".to_owned(), raw);
	serde_json::to_string(&parsed).unwrap_or_else(|_| spec.to_owned())
}

/// `/dns4/host/tcp/443/wss/p2p/id` -> `/ip4/127.0.0.1/tcp/<local>/ws/p2p/id`.
///
/// `None` for anything that is not a DNS-addressed secure WebSocket carrying a
/// peer id, including the plain `/ws` and raw `/tcp` forms the native platform
/// already dials. The peer id is required: without it the noise handshake has
/// no identity to check the remote against, which is the property that makes
/// relaying the transport safe.
fn tunnel_address(address: &str) -> Option<String> {
	let parts: Vec<&str> = address.split('/').collect();
	// ["", scheme, host, "tcp", port, marker, ...]
	let (scheme, host, port) = match parts.get(1..5)? {
		[scheme, host, "tcp", port] => (*scheme, *host, port.parse::<u16>().ok()?),
		_ => return None,
	};
	if !matches!(scheme, "dns" | "dns4" | "dns6") || host.is_empty() {
		return None;
	}
	// Both spellings of secure WebSocket: `/wss` and `/tls/ws`.
	let tail = match parts.get(5..)? {
		["wss", rest @ ..] => rest,
		["tls", "ws", rest @ ..] => rest,
		_ => return None,
	};
	if !tail.contains(&"p2p") {
		return None;
	}
	// The upstream authority has to be a name TLS can be verified against.
	ServerName::try_from(host.to_owned()).ok()?;

	let local = ensure_tunnel(host, port)?;
	let mut rewritten = format!("/ip4/127.0.0.1/tcp/{local}/ws");
	for segment in tail {
		rewritten.push('/');
		rewritten.push_str(segment);
	}
	Some(rewritten)
}

/// The loopback port relaying to `host:port`, starting a listener on first use.
///
/// A cached entry whose accept loop has ended is replaced rather than handed out
/// again, since returning that port would make every later dial fail with
/// nothing listening. Liveness is read from the flag the loop clears on exit
/// rather than by connecting: a probe connection is itself accepted and relayed,
/// so it would cost an upstream TLS handshake on every reuse, and it would block
/// while this function holds the tunnel map.
fn ensure_tunnel(host: &str, port: u16) -> Option<u16> {
	let tunnels = TUNNELS.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = lock(tunnels);
	let key = (host.to_owned(), port);
	if let Some(tunnel) = guard.get(&key) &&
		tunnel.alive.load(Ordering::Acquire)
	{
		return Some(tunnel.local);
	}

	// Bind synchronously so the caller learns the port before the spec is
	// rewritten; the accept loop then runs on its own runtime.
	let listener = match std::net::TcpListener::bind(("127.0.0.1", 0)) {
		Ok(listener) => listener,
		Err(error) => {
			tracing::warn!(%error, "could not bind a loopback tunnel; leaving the bootnode as wss");
			return None;
		},
	};
	let local = listener.local_addr().ok()?.port();
	let upstream_host = host.to_owned();
	let alive = Arc::new(AtomicBool::new(true));
	let loop_alive = Arc::clone(&alive);
	if let Err(error) =
		std::thread::Builder::new().name(format!("truapi-wss-{local}")).spawn(move || {
			let _alive = AliveUntilDropped(loop_alive);
			run_tunnel(listener, upstream_host, port);
		}) {
		tracing::warn!(%error, "could not start a tunnel thread; leaving the bootnode as wss");
		return None;
	}

	guard.insert(key, Tunnel { local, alive });
	tracing::debug!(host, port, local, "tunnelling a wss bootnode over loopback ws");
	Some(local)
}

/// Accept loop for one upstream, on a dedicated single-threaded runtime.
///
/// Accept errors are transient (a descriptor limit, a peer that vanished between
/// the SYN and the accept), so the loop reports and continues. Only losing the
/// listener itself ends it, which drops [`AliveUntilDropped`] and is how
/// [`ensure_tunnel`] learns the tunnel is gone.
fn run_tunnel(listener: std::net::TcpListener, host: String, port: u16) {
	let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
		Ok(runtime) => runtime,
		Err(error) => {
			tracing::warn!(%error, host, port, "wss tunnel runtime failed to start");
			return;
		},
	};
	runtime.block_on(async move {
		if let Err(error) = listener.set_nonblocking(true) {
			tracing::warn!(%error, host, port, "wss tunnel listener is unusable");
			return;
		}
		let listener = match TcpListener::from_std(listener) {
			Ok(listener) => listener,
			Err(error) => {
				tracing::warn!(%error, host, port, "wss tunnel listener is unusable");
				return;
			},
		};
		loop {
			let downstream = match listener.accept().await {
				Ok((downstream, _)) => downstream,
				Err(error) => {
					tracing::debug!(%error, "wss tunnel accept failed");
					continue;
				},
			};
			let host = host.clone();
			tokio::spawn(async move {
				if let Err(error) = relay(downstream, &host, port).await {
					tracing::debug!(%error, "wss tunnel connection ended");
				}
			});
		}
	});
}

/// Relay one accepted connection to `host:port` over TLS, repairing the `Host`
/// header of the WebSocket upgrade on the way through.
async fn relay(
	mut downstream: TcpStream,
	host: &str,
	port: u16,
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
	let upstream =
		tokio::time::timeout(UPSTREAM_TIMEOUT, TcpStream::connect((host, port))).await??;
	let server_name = ServerName::try_from(host.to_owned())?;
	let mut upstream = tokio::time::timeout(
		UPSTREAM_TIMEOUT,
		TlsConnector::from(tls_config()).connect(server_name, upstream),
	)
	.await??;

	let upgrade = tokio::time::timeout(UPSTREAM_TIMEOUT, read_upgrade(&mut downstream)).await??;
	upstream.write_all(&rewrite_host(&upgrade, host, port)).await?;

	copy_bidirectional(&mut downstream, &mut upstream).await?;
	Ok(())
}

/// Read the WebSocket upgrade request, up to and including its blank line.
async fn read_upgrade(
	downstream: &mut TcpStream,
) -> Result<Vec<u8>, Box<dyn core::error::Error + Send + Sync>> {
	let mut buffer = Vec::with_capacity(512);
	let mut byte = [0u8; 1];
	while !buffer.ends_with(b"\r\n\r\n") {
		if buffer.len() >= MAX_UPGRADE_BYTES {
			return Err("the upgrade request exceeded the tunnel's header budget".into());
		}
		if downstream.read(&mut byte).await? == 0 {
			return Err("the connection closed before the upgrade request finished".into());
		}
		buffer.push(byte[0]);
	}
	Ok(buffer)
}

/// Replace the `Host` header with the upstream authority.
///
/// smoldot writes the address it dialled, which after the rewrite is the
/// loopback one. A bootnode behind name-based virtual hosting routes on this
/// header, so forwarding it unchanged is what makes the tunnel return 404.
fn rewrite_host(upgrade: &[u8], host: &str, port: u16) -> Vec<u8> {
	let mut out = Vec::with_capacity(upgrade.len() + host.len());
	for line in upgrade.split_inclusive(|byte| *byte == b'\n') {
		let is_host = line
			.split(|byte| *byte == b':')
			.next()
			.is_some_and(|name| name.eq_ignore_ascii_case(b"host"));
		if is_host {
			out.extend_from_slice(format!("Host: {host}:{port}\r\n").as_bytes());
		} else {
			out.extend_from_slice(line);
		}
	}
	out
}

/// One shared client config; webpki roots rather than the platform verifier so
/// the behaviour is identical on every native target.
fn tls_config() -> Arc<ClientConfig> {
	static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
	Arc::clone(CONFIG.get_or_init(|| {
		let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
		Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
	}))
}

#[cfg(test)]
mod tests {
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
		let rewritten = String::from_utf8(rewrite_host(upgrade, "example.com", 443))
			.expect("headers stay utf-8");
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
}
