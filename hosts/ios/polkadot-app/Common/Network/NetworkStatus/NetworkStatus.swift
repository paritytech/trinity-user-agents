import Foundation
import SubstrateSdk

enum NetworkStatus: Equatable {
    case waitingForNetwork
    case connecting
    case connected
}

extension WebSocketEngine.State {
    var isConnected: Bool {
        if case .connected = self {
            true
        } else {
            false
        }
    }
}
