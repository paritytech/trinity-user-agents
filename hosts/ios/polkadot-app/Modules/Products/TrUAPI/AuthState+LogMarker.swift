import TrUAPIHost

extension AuthState {
    /// The marker the Android host logs for the same state, so `truapi.auth` lines read alike on both.
    var logMarker: String {
        switch self {
        case .disconnected:
            "disconnected"
        case .pairing:
            "pairing"
        case .connected:
            "connected"
        case let .loginFailed(_, reason):
            "login_failed: \(reason)"
        case .authenticating:
            "authenticating"
        }
    }
}
