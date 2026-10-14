import Foundation
import SubstrateSdk
import XcmTransfer
import ChainRegistry

final class BalanceChangeDetector {
    let walletRemoteSubscription: WalletRemoteSubscriptionProtocol
    let accountId: AccountId
    let chainAsset: ChainAsset
    let logger: LoggerProtocol

    init(
        accountId: AccountId,
        chainAsset: ChainAsset,
        walletRemoteSubscription: WalletRemoteSubscriptionProtocol,
        logger: LoggerProtocol
    ) {
        self.accountId = accountId
        self.chainAsset = chainAsset
        self.walletRemoteSubscription = walletRemoteSubscription
        self.logger = logger
    }
}

extension BalanceChangeDetector: BalanceChangeDetecting {
    func subscribe(
        notifyingIn queue: DispatchQueue,
        closure: @escaping BalanceChangeDetectingClosure
    ) {
        walletRemoteSubscription.subscribeBalance(
            for: accountId,
            chainAsset: chainAsset,
            callbackQueue: queue
        ) { [logger] result in
            switch result {
            case let .success(update):
                if let blockHash = update.blockHash {
                    closure(.success(blockHash))
                } else {
                    logger.warning("No block hash found in update")
                }
            case let .failure(error):
                closure(.failure(error))
            }
        }
    }

    func unsubscribe() {
        walletRemoteSubscription.unsubscribe()
    }
}

final class BalanceChangeDetectorFactory {
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension BalanceChangeDetectorFactory: BalanceChangeDetectorFactoryProtocol {
    func createDetector(
        for accountId: AccountId,
        chainAsset: ChainAssetProtocol
    ) -> BalanceChangeDetecting? {
        guard let chainAssetModel = chainAsset as? ChainAsset else {
            return nil
        }

        return BalanceChangeDetector(
            accountId: accountId,
            chainAsset: chainAssetModel,
            walletRemoteSubscription: WalletRemoteSubscription(
                chainRegistry: chainRegistry,
                operationQueue: operationQueue,
                logger: logger
            ),
            logger: logger
        )
    }
}
