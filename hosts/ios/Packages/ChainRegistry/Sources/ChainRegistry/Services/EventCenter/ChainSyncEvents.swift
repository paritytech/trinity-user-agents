import Foundation
import EventCenter

public struct ChainSyncDidStart: EventProtocol {
    public init() {}

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processChainSyncDidStart(event: self)
    }
}

public struct ChainSyncDidComplete: EventProtocol {
    public let newOrUpdatedChains: [ChainModel]
    public let removedChains: [ChainModel]

    public init(newOrUpdatedChains: [ChainModel], removedChains: [ChainModel]) {
        self.newOrUpdatedChains = newOrUpdatedChains
        self.removedChains = removedChains
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processChainSyncDidComplete(event: self)
    }
}

public struct ChainSyncDidFail: EventProtocol {
    public let error: Error

    public init(error: Error) {
        self.error = error
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processChainSyncDidFail(event: self)
    }
}
