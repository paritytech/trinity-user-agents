import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import AssetsManagement
import ChainRegistry
import SubstrateSdkExt

final class AccountInfoSubscription {
    let remoteStorageKey: Data
    let chainAssetId: ChainAssetId
    let accountId: AccountId
    let chainRegistry: ChainRegistryProtocol
    let balanceUpdateProcessor: BalanceUpdateProcessing
    let logger: LoggerProtocol
    let operationQueue: OperationQueue

    init(
        chainAssetId: ChainAssetId,
        accountId: AccountId,
        chainRegistry: ChainRegistryProtocol,
        balanceUpdateProcessor: BalanceUpdateProcessing,
        remoteStorageKey: Data,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainAssetId = chainAssetId
        self.accountId = accountId
        self.chainRegistry = chainRegistry
        self.balanceUpdateProcessor = balanceUpdateProcessor
        self.remoteStorageKey = remoteStorageKey
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

private extension AccountInfoSubscription {
    func createDecodingOperationWrapper(
        _ item: Data?,
        chainAssetId: ChainAssetId
    ) -> CompoundOperationWrapper<SystemPallet.AccountInfo?> {
        guard let runtimeProvider = chainRegistry.getRuntimeProvider(for: chainAssetId.chainId) else {
            return CompoundOperationWrapper.createWithError(
                ChainRegistryError.runtimeMetadaUnavailable
            )
        }

        let codingFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()
        let decodingOperation = StorageFallbackDecodingOperation<SystemPallet.AccountInfo>(
            path: SystemPallet.accountPath,
            data: item
        )

        decodingOperation.configurationBlock = {
            do {
                decodingOperation.codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
            } catch {
                decodingOperation.result = .failure(error)
            }
        }

        decodingOperation.addDependency(codingFactoryOperation)

        decodingOperation.addDependency(codingFactoryOperation)

        return CompoundOperationWrapper(
            targetOperation: decodingOperation,
            dependencies: [codingFactoryOperation]
        )
    }

    func createChangesOperation(
        dependingOn decodingWrapper: CompoundOperationWrapper<SystemPallet.AccountInfo?>,
        chainAssetId: ChainAssetId,
        accountId: AccountId
    ) -> BaseOperation<AssetBalance> {
        ClosureOperation<AssetBalance> {
            let account = try decodingWrapper.targetOperation.extractNoCancellableResultData()

            return AssetBalance(
                accountInfo: account,
                chainAssetId: chainAssetId,
                accountId: accountId
            )
        }
    }

    func decodeAndSaveAccountInfo(
        _ item: Data?,
        chainAssetId: ChainAssetId,
        accountId: AccountId,
        blockHash: Data?
    ) {
        let decodingWrapper = createDecodingOperationWrapper(
            item,
            chainAssetId: chainAssetId
        )

        let changesOperation = createChangesOperation(
            dependingOn: decodingWrapper,
            chainAssetId: chainAssetId,
            accountId: accountId
        )

        let saveOperation = ClosureOperation<Void> { [weak self] in
            let balance = try changesOperation.extractNoCancellableResultData()
            self?.balanceUpdateProcessor.process(balance: balance, blockHash: blockHash)
        }

        changesOperation.addDependency(decodingWrapper.targetOperation)
        saveOperation.addDependency(changesOperation)

        let operations = decodingWrapper.allOperations + [changesOperation, saveOperation]

        logger.debug("Asset info processing: \(chainAssetId)")

        operationQueue.addOperations(operations, waitUntilFinished: false)
    }
}

extension AccountInfoSubscription: StorageChildSubscribing {
    func processUpdate(_ data: Data?, blockHash: Data?) {
        logger.debug("Did account info update")

        decodeAndSaveAccountInfo(
            data,
            chainAssetId: chainAssetId,
            accountId: accountId,
            blockHash: blockHash
        )
    }
}
