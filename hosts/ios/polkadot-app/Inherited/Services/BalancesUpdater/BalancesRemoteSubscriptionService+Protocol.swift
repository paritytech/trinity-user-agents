import Foundation
import SubstrateSdk
import ChainRegistry

extension BalanceRemoteSubscriptionService: BalanceRemoteSubscriptionServiceProtocol {
    private func createCacheKey(from accountId: AccountId, chainId: ChainModel.Id) -> String {
        "balances-\(accountId.toHex())-\(chainId)"
    }

    private func createCacheKey(from accountId: AccountId, chainAssetId: ChainAssetId) -> String {
        "balances-\(accountId.toHex())-\(chainAssetId.chainId)-\(chainAssetId.assetId)"
    }

    func attachToBalances(
        for accountId: AccountId,
        chain: ChainModel,
        onlyFor assetIds: Set<AssetModel.Id>?,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    ) -> UUID? {
        let subscriptionSettingsList = prepareSubscriptionRequests(
            from: accountId,
            chain: chain,
            onlyFor: assetIds
        )

        guard !subscriptionSettingsList.isEmpty else {
            return nil
        }

        let cacheKey = createCacheKey(from: accountId, chainId: chain.chainId)

        let requests = subscriptionSettingsList.map(\.request)
        let handlersStore = subscriptionSettingsList.reduce(
            into: [String: RemoteSubscriptionHandleFactoryProtocol]()
        ) { accum, settings in
            accum[settings.request.localKey] = settings.handlingFactory
        }

        let handlingFactory = BalanceRemoteSubscriptionHandlingProxy(
            store: handlersStore,
            logger: logger
        )

        return attachToSubscription(
            with: .init(
                requests: requests,
                chainId: chain.chainId,
                cacheKey: cacheKey,
                handlingFactory: handlingFactory
            ),
            queue: queue,
            closure: closure
        )
    }

    func detachFromBalances(
        for subscriptionId: UUID,
        accountId: AccountId,
        chainId: ChainModel.Id,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    ) {
        let cacheKey = createCacheKey(from: accountId, chainId: chainId)
        detachFromSubscription(cacheKey, subscriptionId: subscriptionId, queue: queue, closure: closure)
    }

    func attachToAssetBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    ) -> UUID? {
        let subscriptionSettingsList = prepareSubscriptionRequests(
            from: accountId,
            chainAsset: chainAsset
        )

        guard !subscriptionSettingsList.isEmpty else {
            return nil
        }

        let cacheKey = createCacheKey(from: accountId, chainAssetId: chainAsset.chainAssetId)

        let requests = subscriptionSettingsList.map(\.request)
        let handlersStore = subscriptionSettingsList.reduce(
            into: [String: RemoteSubscriptionHandleFactoryProtocol]()
        ) { accum, settings in
            accum[settings.request.localKey] = settings.handlingFactory
        }

        let handlingFactory = BalanceRemoteSubscriptionHandlingProxy(store: handlersStore, logger: logger)

        return attachToSubscription(
            with: .init(
                requests: requests,
                chainId: chainAsset.chain.chainId,
                cacheKey: cacheKey,
                handlingFactory: handlingFactory
            ),
            queue: queue,
            closure: closure
        )
    }

    func detachFromAssetBalance(
        for subscriptionId: UUID,
        accountId: AccountId,
        chainAssetId: ChainAssetId,
        queue: DispatchQueue?,
        closure: RemoteSubscriptionClosure?
    ) {
        let cacheKey = createCacheKey(from: accountId, chainAssetId: chainAssetId)
        detachFromSubscription(cacheKey, subscriptionId: subscriptionId, queue: queue, closure: closure)
    }
}
