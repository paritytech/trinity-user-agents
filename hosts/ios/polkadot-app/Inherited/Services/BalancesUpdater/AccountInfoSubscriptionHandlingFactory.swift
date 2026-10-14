import Foundation
import Operation_iOS
import SubstrateStorageSubscription
import ChainRegistry
import SubstrateSdkExt

final class AccountInfoSubscriptionHandlingFactory: RemoteSubscriptionHandleFactoryProtocol {
    struct LocalStorageKeys {
        let account: String
    }

    let localKeys: LocalStorageKeys
    let factory: NativeTokenSubscriptionFactoryProtocol

    init(
        localKeys: LocalStorageKeys,
        factory: NativeTokenSubscriptionFactoryProtocol
    ) {
        self.localKeys = localKeys
        self.factory = factory
    }

    func createHandler(remoteStorageKey: Data, localStorageKey: String) -> StorageChildSubscribing {
        factory.createAccountInfoSubscription(remoteStorageKey: remoteStorageKey, localStorageKey: localStorageKey)
    }
}
