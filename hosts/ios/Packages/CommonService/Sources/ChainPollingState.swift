import Foundation
import SubstrateSdk
import SubstrateStorageSubscription

public struct ChainPollingState: Equatable {
    public typealias TChange = BatchSubscriptionHandler

    public let blockHash: BlockHashData?

    public init(blockHash: BlockHashData?) {
        self.blockHash = blockHash
    }
}

extension ChainPollingState: ObservableSubscriptionStateProtocol {
    public init(change: TChange) throws {
        blockHash = change.blockHash
    }

    public func merging(change: TChange) -> Self {
        ChainPollingState(blockHash: change.blockHash)
    }
}

public protocol ChainPollingStateStoring: BaseObservableStateStoreProtocol where RemoteState == ChainPollingState {}

public final class ChainPollingStateStore: ObservableSubscriptionStateStore<ChainPollingState> {
    override public func getRequests() throws -> [BatchStorageSubscriptionRequest] {
        [
            BatchStorageSubscriptionRequest(
                innerRequest: UnkeyedSubscriptionRequest(
                    storagePath: SystemPallet.blockNumberPath,
                    localKey: ""
                ),
                mappingKey: nil
            )
        ]
    }
}

extension ChainPollingStateStore: ChainPollingStateStoring {}
