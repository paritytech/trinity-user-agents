import Foundation
import SubstrateSdk
import SubstrateSdkExt

public protocol ChainConnection: JSONRPCEngine & ConnectionAutobalancing & ConnectionStateReporting {
    func connect()
    func disconnect(_ force: Bool)
}

extension WebSocketEngine: ChainConnection {
    public func connect() {
        connectIfNeeded()
    }

    public func disconnect(_ force: Bool) {
        disconnectIfNeeded(force)
    }
}

public protocol OneShotConnection: JSONRPCEngine & ConnectionAutobalancing {}

extension HTTPEngine: OneShotConnection {}
