/// The single value the status ring renders, derived from connection state and liveness.
/// `resolve` is total and clock-free: everything time-dependent — the dwell, liveness —
/// stays with the provider that already owns state.
public enum ChainStatusIndication: Hashable {
    case normal
    case outage(liveness: Double)
    case dead

    private static let outageThreshold: Double = 5.0 / 6.0

    public static func resolve(state: ChainConnectionState, liveness: Double?) -> ChainStatusIndication {
        switch state {
        case .connected:
            if let liveness, liveness < outageThreshold {
                return .outage(liveness: liveness)
            }
            return .normal
        case .connecting,
             .offline:
            return .dead
        }
    }
}
