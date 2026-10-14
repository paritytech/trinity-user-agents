import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageSubscription
import SubstrateStorageQuery
import SDKLogger

public final class StorageSubscriptionContainer: WebSocketSubscribing {
    public let children: [StorageChildSubscribing]
    public let engine: JSONRPCEngine
    public let logger: SDKLoggerProtocol

    private var subscriptionId: UInt16?

    public init(
        engine: JSONRPCEngine,
        children: [StorageChildSubscribing],
        logger: SDKLoggerProtocol
    ) {
        self.children = children
        self.engine = engine
        self.logger = logger

        subscribe()
    }

    deinit {
        unsubscribe()
    }

    private func subscribe() {
        do {
            let storageKeys = children.map { $0.remoteStorageKey.toHex(includePrefix: true) }

            let updateClosure: (StorageSubscriptionUpdate) -> Void = { [weak self] update in
                self?.handleUpdate(update.params.result)
            }

            let failureClosure: (Error, Bool) -> Void = { [weak self] error, unsubscribed in
                self?.logger.error("Did receive subscription error: \(error) \(unsubscribed)")
            }

            subscriptionId = try engine.subscribe(
                RPCMethod.storageSubscribe,
                params: [storageKeys],
                updateClosure: updateClosure,
                failureClosure: failureClosure
            )
        } catch {
            logger.error("Can't subscribe to storage: \(error)")
        }
    }

    private func unsubscribe() {
        if let identifier = subscriptionId {
            engine.cancelForIdentifier(identifier)
        }
    }

    private func handleUpdate(_ update: StorageUpdate) {
        let updateData = StorageUpdateData(update: update)

        for change in updateData.changes {
            let childrenToNotify = children.filter { $0.remoteStorageKey == change.key }

            for item in childrenToNotify {
                item.processUpdate(change.value, blockHash: updateData.blockHash)
            }
        }
    }
}
