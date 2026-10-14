import Foundation
import SubstrateSdk
import Operation_iOS
import CommonService
import ChainRegistry

protocol OrmlHydrationEvmWalletSyncFactoryProtocol {
    func createSyncService(for chainId: ChainModel.Id, accountId: AccountId) -> ApplicationServiceProtocol
}

final class OrmlHydrationEvmWalletSyncFactory {
    let chainRegistry: ChainRegistryProtocol
    let balanceUpdateProcessor: BalanceUpdateProcessing
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        balanceUpdateProcessor: BalanceUpdateProcessing,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.balanceUpdateProcessor = balanceUpdateProcessor
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension OrmlHydrationEvmWalletSyncFactory: OrmlHydrationEvmWalletSyncFactoryProtocol {
    func createSyncService(
        for chainId: ChainModel.Id,
        accountId: AccountId
    ) -> ApplicationServiceProtocol {
        OrmlHydrationEvmBalanceSyncService(
            chainId: chainId,
            accountId: accountId,
            chainRegistry: chainRegistry,
            balanceUpdateProcessor: balanceUpdateProcessor,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
