import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateStorageSubscription
import AssetsManagement
import ChainRegistry

protocol BalanceRemoteSubscriptionServiceProtocol {
    func attachToBalances(
        for accountId: AccountId,
        chain: ChainModel,
        onlyFor assetIds: Set<AssetModel.Id>?,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    ) -> UUID?

    func detachFromBalances(
        for subscriptionId: UUID,
        accountId: AccountId,
        chainId: ChainModel.Id,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    )

    func attachToAssetBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    ) -> UUID?

    func detachFromAssetBalance(
        for subscriptionId: UUID,
        accountId: AccountId,
        chainAssetId: ChainAssetId,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    )
}

final class BalanceRemoteSubscriptionService: RemoteSubscriptionService {
    struct SubscriptionSettings {
        let request: SubscriptionRequestProtocol
        let handlingFactory: RemoteSubscriptionHandleFactoryProtocol
    }

    let subscriptionHandlingFactory: BalanceRemoteHandlingFactoryProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        subscriptionHandlingFactory: BalanceRemoteHandlingFactoryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.subscriptionHandlingFactory = subscriptionHandlingFactory

        super.init(chainRegistry: chainRegistry, operationQueue: operationQueue, logger: logger)
    }

    private func prepareNativeAssetSubscriptionRequests(
        from accountId: AccountId,
        chainAsset: ChainAsset
    ) throws -> [SubscriptionSettings] {
        let storageKeyFactory = LocalStorageKeyFactory()
        let chainId = chainAsset.chain.chainId

        // account
        let accountStoragePath = SystemPallet.accountPath
        let accountLocalKey = try storageKeyFactory.createFromStoragePath(
            accountStoragePath,
            accountId: accountId,
            chainId: chainId
        )

        let accountRequest = MapSubscriptionRequest(
            storagePath: accountStoragePath,
            localKey: accountLocalKey
        ) {
            BytesCodable(wrappedValue: accountId)
        }

        let handlerFactory = subscriptionHandlingFactory.createNative(
            for: accountId,
            chainAssetId: chainAsset.chainAssetId,
            params: .init(
                accountLocalStorageKey: accountRequest.localKey
            )
        )

        return [accountRequest].map {
            SubscriptionSettings(request: $0, handlingFactory: handlerFactory)
        }
    }

    private func prepareAssetsPalletSubscriptionRequests(
        from accountId: AccountId,
        chainAsset: ChainAsset
    ) throws -> [SubscriptionSettings] {
        guard let extras = try chainAsset.asset.typeExtras?.map(to: StatemineAssetExtras.self) else {
            return []
        }

        let chainId = chainAsset.chain.chainId

        let localKeyFactory = LocalStorageKeyFactory()

        let assetId = extras.assetId
        let accountStoragePath = AssetsPallet.accountPath(from: extras.palletName)
        let accountLocalKey = try localKeyFactory.createFromStoragePath(
            accountStoragePath,
            encodableElements: [assetId, accountId],
            chainId: chainId
        )

        let accountRequest = DoubleMapSubscriptionRequest(
            storagePath: accountStoragePath,
            localKey: accountLocalKey,
            keyParamClosure: { (assetId, accountId) },
            param1Encoder: AssetsPalletSerializer.subscriptionKeyEncoder(for: assetId),
            param2Encoder: nil
        )

        let detailsStoragePath = AssetsPallet.detailsPath(from: extras.palletName)
        let detailsLocalKey = try localKeyFactory.createFromStoragePath(
            detailsStoragePath,
            encodableElement: assetId,
            chainId: chainId
        )

        let detailsRequest = MapSubscriptionRequest(
            storagePath: detailsStoragePath,
            localKey: detailsLocalKey,
            keyParamClosure: { assetId },
            paramEncoder: AssetsPalletSerializer.subscriptionKeyEncoder(for: assetId)
        )

        let handlerFactory = subscriptionHandlingFactory.createAssetsPallet(
            for: accountId,
            chainAssetId: chainAsset.chainAssetId,
            params: .init(assetAccountKey: accountLocalKey, assetDetailsKey: detailsLocalKey, extras: extras)
        )

        return [
            SubscriptionSettings(request: accountRequest, handlingFactory: handlerFactory),
            SubscriptionSettings(request: detailsRequest, handlingFactory: handlerFactory)
        ]
    }

    private func prepareOrmlSubscriptionRequests(
        from accountId: AccountId,
        chainAsset: ChainAsset
    ) throws -> [SubscriptionSettings] {
        guard let tokenExtras = try chainAsset.asset.typeExtras?.map(to: OrmlTokenExtras.self) else {
            return []
        }

        let currencyId = try Data(hexString: tokenExtras.currencyIdScale)
        let chainId = chainAsset.chain.chainId

        let accountStoragePath = OrmlPallet.ormlTokenAccount

        let storageKeyFactory = LocalStorageKeyFactory()
        let accountLocalKey = try storageKeyFactory.createFromStoragePath(
            accountStoragePath,
            encodableElement: accountId + currencyId,
            chainId: chainId
        )

        let accountRequest = DoubleMapSubscriptionRequest(
            storagePath: accountStoragePath,
            localKey: accountLocalKey,
            keyParamClosure: { (accountId, currencyId) },
            param1Encoder: nil,
            param2Encoder: { $0 }
        )

        let handlerFactory = subscriptionHandlingFactory.createOrml(
            for: accountId,
            chainAssetId: chainAsset.chainAssetId,
            params: .init(accountLocalStorageKey: accountLocalKey)
        )

        return [
            SubscriptionSettings(request: accountRequest, handlingFactory: handlerFactory)
        ]
    }

    func prepareSubscriptionRequests(
        from accountId: AccountId,
        chainAsset: ChainAsset
    ) -> [SubscriptionSettings] {
        do {
            switch AssetType(rawType: chainAsset.asset.type) {
            case .native:
                return try prepareNativeAssetSubscriptionRequests(
                    from: accountId,
                    chainAsset: chainAsset
                )
            case .statemine:
                return try prepareAssetsPalletSubscriptionRequests(
                    from: accountId,
                    chainAsset: chainAsset
                )
            case .orml:
                return try prepareOrmlSubscriptionRequests(
                    from: accountId,
                    chainAsset: chainAsset
                )
            case .none,
                 .ormlHydrationEvm:
                return []
            }
        } catch {
            logger.error("Can't create request: \(error)")
            return []
        }
    }

    func prepareSubscriptionRequests(
        from accountId: AccountId,
        chain: ChainModel,
        onlyFor assetIds: Set<AssetModel.Id>?
    ) -> [SubscriptionSettings] {
        let chainAssets =
            if let assetIds {
                chain.chainAssets().filter { assetIds.contains($0.asset.assetId) }
            } else {
                chain.chainAssets()
            }

        return chainAssets.flatMap { chainAsset in
            prepareSubscriptionRequests(
                from: accountId,
                chainAsset: chainAsset
            )
        }
    }
}
