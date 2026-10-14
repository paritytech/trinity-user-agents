import Foundation
import SubstrateSdk
import AsyncExtensions
import AssetsManagement
import ChainRegistry
import EventCenter

protocol BalanceTrackingFactoryProtocol {
    func trackAll(for wallet: MetaAccountModelProtocol) -> AnyAsyncSequence<AssetBalance>
    func trackAccountAsset(_ accountId: AccountId, chainAsset: ChainAsset) -> AnyAsyncSequence<AssetBalance>
}

final class BalanceTrackingFactory {
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let eventCenter: EventCenterProtocol
    let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        eventCenter: EventCenterProtocol = EventCenter.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
        self.eventCenter = eventCenter
        self.logger = logger
    }
}

extension BalanceTrackingFactory: BalanceTrackingFactoryProtocol {
    func trackAll(for wallet: MetaAccountModelProtocol) -> AnyAsyncSequence<AssetBalance> {
        let deps = BalanceTrackingDeps(
            wallet: wallet,
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            eventCenter: eventCenter,
            logger: logger
        )

        return BalanceTracking.track(with: deps)
    }

    func trackAccountAsset(_ accountId: AccountId, chainAsset: ChainAsset) -> AnyAsyncSequence<AssetBalance> {
        let deps = AccountBalanceTrackingDeps(
            accountId: accountId,
            asset: chainAsset,
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            logger: logger
        )

        return BalanceTracking.trackAccountAsset(with: deps)
    }
}
