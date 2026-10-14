import Foundation
import ChainRegistry
import Operation_iOS
import SubstrateStorageSubscription
import SubstrateSdkExt

final class BalanceRemoteSubscriptionHandlingProxy {
    let store: [String: RemoteSubscriptionHandleFactoryProtocol]
    let logger: LoggerProtocol

    init(store: [String: RemoteSubscriptionHandleFactoryProtocol], logger: LoggerProtocol) {
        self.store = store
        self.logger = logger
    }
}

extension BalanceRemoteSubscriptionHandlingProxy: RemoteSubscriptionHandleFactoryProtocol {
    func createHandler(remoteStorageKey: Data, localStorageKey: String) -> StorageChildSubscribing {
        if let handler = store[localStorageKey] {
            handler.createHandler(remoteStorageKey: remoteStorageKey, localStorageKey: localStorageKey)
        } else {
            EmptyHandlingStorageSubscription(remoteStorageKey: remoteStorageKey, logger: logger)
        }
    }
}
