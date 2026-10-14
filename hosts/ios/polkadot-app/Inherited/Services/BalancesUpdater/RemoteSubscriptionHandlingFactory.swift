import Foundation
import ChainRegistry
import Operation_iOS
import SubstrateStorageSubscription
import SubstrateSdkExt

protocol RemoteSubscriptionHandleFactoryProtocol {
    func createHandler(remoteStorageKey: Data, localStorageKey: String) -> StorageChildSubscribing
}
