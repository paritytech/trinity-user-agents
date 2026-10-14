import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageSubscription
import ChainRegistry
import SubstrateSdkExt

protocol OrmlTokenSubscriptionFactoryProtocol {
    func createOrmlAccountSubscription(remoteStorageKey: Data, localStorageKey: String) -> StorageChildSubscribing
}

protocol NativeTokenSubscriptionFactoryProtocol {
    func createAccountInfoSubscription(remoteStorageKey: Data, localStorageKey: String) -> StorageChildSubscribing
}

// MARK: - OrmlTokenSubscriptionFactoryProtocol

final class TokenSubscriptionFactory: OrmlTokenSubscriptionFactoryProtocol {
    let chainAssetId: ChainAssetId
    let accountId: AccountId
    let chainRegistry: ChainRegistryProtocol
    let balanceUpdateProcessor: BalanceUpdateProcessing
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        chainAssetId: ChainAssetId,
        accountId: AccountId,
        chainRegistry: ChainRegistryProtocol,
        balanceUpdateProcessor: BalanceUpdateProcessing,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainAssetId = chainAssetId
        self.accountId = accountId
        self.chainRegistry = chainRegistry
        self.balanceUpdateProcessor = balanceUpdateProcessor
        self.operationQueue = operationQueue
        self.logger = logger
    }

    func createOrmlAccountSubscription(remoteStorageKey: Data, localStorageKey _: String) -> StorageChildSubscribing {
        OrmlAccountSubscription(
            chainAssetId: chainAssetId,
            accountId: accountId,
            chainRegistry: chainRegistry,
            remoteStorageKey: remoteStorageKey,
            balanceUpdateProcessor: balanceUpdateProcessor,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

// MARK: - NativeTokenSubscriptionFactoryProtocol

extension TokenSubscriptionFactory: NativeTokenSubscriptionFactoryProtocol {
    func createAccountInfoSubscription(remoteStorageKey: Data, localStorageKey _: String) -> StorageChildSubscribing {
        AccountInfoSubscription(
            chainAssetId: chainAssetId,
            accountId: accountId,
            chainRegistry: chainRegistry,
            balanceUpdateProcessor: balanceUpdateProcessor,
            remoteStorageKey: remoteStorageKey,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
