import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

enum BalanceRemoteSubscriptionHandlingParams {
    struct BalancesPallet {
        let accountLocalStorageKey: String
    }

    struct OrmlPallet {
        let accountLocalStorageKey: String
    }

    struct AssetsPallet {
        let assetAccountKey: String
        let assetDetailsKey: String
        let extras: StatemineAssetExtras
    }
}

protocol BalanceRemoteHandlingFactoryProtocol {
    func createNative(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        params: BalanceRemoteSubscriptionHandlingParams.BalancesPallet
    ) -> RemoteSubscriptionHandleFactoryProtocol

    func createOrml(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        params: BalanceRemoteSubscriptionHandlingParams.OrmlPallet
    ) -> RemoteSubscriptionHandleFactoryProtocol

    func createAssetsPallet(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        params: BalanceRemoteSubscriptionHandlingParams.AssetsPallet
    ) -> RemoteSubscriptionHandleFactoryProtocol
}

final class BalanceRemoteSubscriptionHandlingFactory {
    let chainRegistry: ChainRegistryProtocol
    let balanceUpdateProcessorFactory: BalanceUpdateProcessorFactoryProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        balanceUpdateProcessorFactory: BalanceUpdateProcessorFactoryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.balanceUpdateProcessorFactory = balanceUpdateProcessorFactory
        self.operationQueue = operationQueue
        self.logger = logger
    }

    private func createTokensSubscriptionFactory(
        for accountId: AccountId,
        chainAssetId: ChainAssetId
    ) -> TokenSubscriptionFactory {
        let balanceProcessor = balanceUpdateProcessorFactory.createProcessor(
            for: accountId,
            chainAssetId: chainAssetId
        )

        return TokenSubscriptionFactory(
            chainAssetId: chainAssetId,
            accountId: accountId,
            chainRegistry: chainRegistry,
            balanceUpdateProcessor: balanceProcessor,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

extension BalanceRemoteSubscriptionHandlingFactory: BalanceRemoteHandlingFactoryProtocol {
    func createNative(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        params: BalanceRemoteSubscriptionHandlingParams.BalancesPallet
    ) -> RemoteSubscriptionHandleFactoryProtocol {
        let innerFactory = createTokensSubscriptionFactory(
            for: accountId,
            chainAssetId: chainAssetId
        )

        return AccountInfoSubscriptionHandlingFactory(
            localKeys: .init(
                account: params.accountLocalStorageKey
            ),
            factory: innerFactory
        )
    }

    func createOrml(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        params _: BalanceRemoteSubscriptionHandlingParams.OrmlPallet
    ) -> RemoteSubscriptionHandleFactoryProtocol {
        let innerFactory = createTokensSubscriptionFactory(
            for: accountId,
            chainAssetId: chainAssetId
        )

        return OrmlTokenSubscriptionHandlingFactory(
            factory: innerFactory
        )
    }

    func createAssetsPallet(
        for accountId: AccountId,
        chainAssetId: ChainAssetId,
        params: BalanceRemoteSubscriptionHandlingParams.AssetsPallet
    ) -> RemoteSubscriptionHandleFactoryProtocol {
        let balanceProcessor = balanceUpdateProcessorFactory.createProcessor(
            for: accountId,
            chainAssetId: chainAssetId
        )

        let balanceUpdater = AssetsBalanceUpdater(
            chainAssetId: chainAssetId,
            accountId: accountId,
            extras: params.extras,
            chainRegistry: chainRegistry,
            balanceUpdateProcessor: balanceProcessor,
            operationQueue: operationQueue,
            logger: logger
        )

        return AssetsSubscriptionHandlingFactory(
            assetAccountKey: params.assetAccountKey,
            assetDetailsKey: params.assetDetailsKey,
            assetBalanceUpdater: balanceUpdater,
            logger: logger
        )
    }
}
