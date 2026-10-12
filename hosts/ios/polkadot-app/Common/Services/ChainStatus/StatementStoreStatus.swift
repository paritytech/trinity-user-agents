import Foundation
import PolkadotUI

enum StatementStoreStatus: Equatable {
    case connecting
    case active
    case unavailable
    case noInternet
}

struct StatementStoreActivity: Equatable {
    var live: Int = 0
    var pending: Int = 0
    var failed: Int = 0

    static let idle = StatementStoreActivity()
}

extension StatementStoreStatus {
    static func resolve(network: NetworkStatus, activity: StatementStoreActivity) -> StatementStoreStatus {
        switch network {
        case .waitingForNetwork:
            return .noInternet
        case .connecting:
            return .connecting
        case .connected:
            break
        }

        if activity.live > 0 {
            return .active
        }

        if activity.failed > 0 {
            return .unavailable
        }

        return activity.pending > 0 ? .connecting : .active
    }

    var connectionState: ChainConnectionState {
        switch self {
        case .active:
            .connected
        case .connecting:
            .connecting
        case .unavailable,
             .noInternet:
            .offline
        }
    }

    var localizedTitle: String {
        switch self {
        case .connecting:
            String(localized: .Common.statementStoreStatusConnecting)
        case .active:
            String(localized: .Common.chainConnectionStatusConnected)
        case .unavailable:
            String(localized: .Common.statementStoreStatusUnavailable)
        case .noInternet:
            String(localized: .Common.chainConnectionStatusOffline)
        }
    }
}
