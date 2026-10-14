import Foundation
import EventCenter

public struct RuntimeCoderCreated: EventProtocol {
    public let chainId: ChainModel.Id

    public init(chainId: ChainModel.Id) {
        self.chainId = chainId
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processRuntimeCoderReady(event: self)
    }
}

public struct RuntimeCoderCreationFailed: EventProtocol {
    public let chainId: ChainModel.Id
    public let error: Error

    public init(chainId: ChainModel.Id, error: Error) {
        self.chainId = chainId
        self.error = error
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processRuntimeCoderCreationFailed(event: self)
    }
}
