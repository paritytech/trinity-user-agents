import Foundation
import SubstrateSdk
import SubstrateStorageSubscription

public struct ObservableSubscriptionState: ObservableSubscriptionStateProtocol {
    public typealias TChange = BatchSubscriptionHandler

    public let blockHash: Data?

    public init(blockHash: Data?) {
        self.blockHash = blockHash
    }

    public init(change: TChange) {
        blockHash = change.blockHash
    }

    public func merging(change: BatchSubscriptionHandler) -> ObservableSubscriptionState {
        .init(blockHash: change.blockHash)
    }
}
