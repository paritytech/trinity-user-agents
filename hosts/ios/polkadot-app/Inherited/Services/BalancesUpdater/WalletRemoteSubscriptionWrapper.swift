import Foundation
import SubstrateSdk
import ChainRegistry
import EventCenter

protocol WalletRemoteSubscriptionWrapperProtocol {
    func subscribe(
        using assetStorageInfo: AssetStorageInfo,
        accountId: AccountId,
        chainAsset: ChainAsset,
        completion: RemoteSubscriptionClosure?
    ) -> UUID?

    func unsubscribe(
        from subscriptionId: UUID,
        assetStorageInfo: AssetStorageInfo,
        accountId: AccountId,
        chainAssetId: ChainAssetId,
        completion: RemoteSubscriptionClosure?
    )
}

final class WalletRemoteSubscriptionWrapper {
    let chainRegistry: ChainRegistryProtocol
    let remoteSubscriptionService: BalanceRemoteSubscriptionServiceProtocol
    let eventCenter: EventCenterProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        remoteSubscriptionService: BalanceRemoteSubscriptionServiceProtocol,
        chainRegistry: ChainRegistryProtocol,
        eventCenter: EventCenterProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.remoteSubscriptionService = remoteSubscriptionService
        self.chainRegistry = chainRegistry
        self.eventCenter = eventCenter
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension WalletRemoteSubscriptionWrapper: WalletRemoteSubscriptionWrapperProtocol {
    func subscribe(
        using assetStorageInfo: AssetStorageInfo,
        accountId: AccountId,
        chainAsset: ChainAsset,
        completion: RemoteSubscriptionClosure?
    ) -> UUID? {
        switch assetStorageInfo {
        case .native,
             .statemine,
             .orml:
            remoteSubscriptionService.attachToAssetBalance(
                for: accountId,
                chainAsset: chainAsset,
                queue: .main,
                closure: completion
            )
        case .ormlHydrationEvm:
            nil
        }
    }

    func unsubscribe(
        from subscriptionId: UUID,
        assetStorageInfo: AssetStorageInfo,
        accountId: AccountId,
        chainAssetId: ChainAssetId,
        completion: RemoteSubscriptionClosure?
    ) {
        switch assetStorageInfo {
        case .native,
             .statemine,
             .orml:
            remoteSubscriptionService.detachFromAssetBalance(
                for: subscriptionId,
                accountId: accountId,
                chainAssetId: chainAssetId,
                queue: .main,
                closure: completion
            )
        case .ormlHydrationEvm:
            break
        }
    }
}
