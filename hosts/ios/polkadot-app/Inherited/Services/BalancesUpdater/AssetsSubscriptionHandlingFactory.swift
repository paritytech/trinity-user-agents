import Foundation
import ChainRegistry
import Operation_iOS
import SubstrateStorageSubscription
import SubstrateSdkExt

final class AssetsSubscriptionHandlingFactory {
    let assetAccountKey: String
    let assetDetailsKey: String
    let assetBalanceUpdater: AssetsBalanceUpdater
    let logger: LoggerProtocol

    init(
        assetAccountKey: String,
        assetDetailsKey: String,
        assetBalanceUpdater: AssetsBalanceUpdater,
        logger: LoggerProtocol
    ) {
        self.assetAccountKey = assetAccountKey
        self.assetDetailsKey = assetDetailsKey
        self.assetBalanceUpdater = assetBalanceUpdater
        self.logger = logger
    }
}

extension AssetsSubscriptionHandlingFactory: RemoteSubscriptionHandleFactoryProtocol {
    func createHandler(remoteStorageKey: Data, localStorageKey: String) -> StorageChildSubscribing {
        if localStorageKey == assetAccountKey {
            AssetAccountSubscription(
                assetBalanceUpdater: assetBalanceUpdater,
                remoteStorageKey: remoteStorageKey,
                logger: logger
            )
        } else {
            AssetDetailsSubscription(
                assetBalanceUpdater: assetBalanceUpdater,
                remoteStorageKey: remoteStorageKey,
                logger: logger
            )
        }
    }
}
