use super::ConnectionTypes;
use smoldot_light::platform::ConnectionType;

const UNSECURE: ConnectionType = ConnectionType::WebSocketIpv6 { remote_is_localhost: false };
const LOCALHOST: ConnectionType = ConnectionType::WebSocketIpv4 { remote_is_localhost: true };
const SECURE: ConnectionType =
	ConnectionType::WebSocketDns { secure: true, remote_is_localhost: false };

#[test]
fn the_default_allows_every_websocket() {
	let types = ConnectionTypes::default();
	assert!(types.allows(UNSECURE) && types.allows(LOCALHOST) && types.allows(SECURE));
	assert!(!types.allows(ConnectionType::TcpIpv4));
}

#[test]
fn unsecure_can_be_refused_alone() {
	let types = ConnectionTypes { unsecure: false, ..ConnectionTypes::default() };
	assert!(!types.allows(UNSECURE));
	assert!(
		!types.allows(ConnectionType::WebSocketDns { secure: false, remote_is_localhost: false })
	);
	assert!(types.allows(LOCALHOST) && types.allows(SECURE));
}
