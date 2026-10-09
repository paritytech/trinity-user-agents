//! The kinds of peer connection the browser light client opens.

#[cfg(any(target_arch = "wasm32", test))]
use smoldot_light::platform::ConnectionType;

/// Which kinds of connection the browser light client opens to a peer. All
/// are allowed by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionTypes {
    /// Secure `wss://` WebSocket.
    pub secure: bool,
    /// Plain `ws://` WebSocket to a localhost peer.
    pub localhost: bool,
    /// Plain `ws://` WebSocket to any other peer.
    pub unsecure: bool,
}

impl Default for ConnectionTypes {
    /// Every kind allowed.
    fn default() -> Self {
        ConnectionTypes {
            secure: true,
            localhost: true,
            unsecure: true,
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl ConnectionTypes {
    /// Whether a connection of `connection_type` may be opened.
    pub(crate) fn allows(&self, connection_type: ConnectionType) -> bool {
        match connection_type {
            ConnectionType::WebSocketDns { secure: true, .. } => self.secure,
            ConnectionType::WebSocketIpv4 {
                remote_is_localhost,
            }
            | ConnectionType::WebSocketIpv6 {
                remote_is_localhost,
            }
            | ConnectionType::WebSocketDns {
                remote_is_localhost,
                ..
            } => {
                if remote_is_localhost {
                    self.localhost
                } else {
                    self.unsecure
                }
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionTypes;
    use smoldot_light::platform::ConnectionType;

    const UNSECURE: ConnectionType = ConnectionType::WebSocketIpv6 {
        remote_is_localhost: false,
    };
    const LOCALHOST: ConnectionType = ConnectionType::WebSocketIpv4 {
        remote_is_localhost: true,
    };
    const SECURE: ConnectionType = ConnectionType::WebSocketDns {
        secure: true,
        remote_is_localhost: false,
    };

    #[test]
    fn the_default_allows_every_websocket() {
        let types = ConnectionTypes::default();
        assert!(types.allows(UNSECURE) && types.allows(LOCALHOST) && types.allows(SECURE));
        assert!(!types.allows(ConnectionType::TcpIpv4));
    }

    #[test]
    fn unsecure_can_be_refused_alone() {
        let types = ConnectionTypes {
            unsecure: false,
            ..ConnectionTypes::default()
        };
        assert!(!types.allows(UNSECURE));
        assert!(!types.allows(ConnectionType::WebSocketDns {
            secure: false,
            remote_is_localhost: false,
        }));
        assert!(types.allows(LOCALHOST) && types.allows(SECURE));
    }
}
