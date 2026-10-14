import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import AssetsManagement
import ChainRegistry

final class AssetsBalanceUpdater {
    let chainAssetId: ChainAssetId
    let accountId: AccountId
    let chainRegistry: ChainRegistryProtocol
    let balanceUpdateProcessor: BalanceUpdateProcessing
    let extras: StatemineAssetExtras
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    private var lastDetailsValue: Data?
    private var receivedDetails: Bool = false

    private var lastAccountValue: Data?
    private var receivedAccount: Bool = false
    private var lastAccountValueHash: Data?

    private let mutex = NSLock()

    init(
        chainAssetId: ChainAssetId,
        accountId: AccountId,
        extras: StatemineAssetExtras,
        chainRegistry: ChainRegistryProtocol,
        balanceUpdateProcessor: BalanceUpdateProcessing,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainAssetId = chainAssetId
        self.accountId = accountId
        self.extras = extras
        self.chainRegistry = chainRegistry
        self.balanceUpdateProcessor = balanceUpdateProcessor
        self.operationQueue = operationQueue
        self.logger = logger
    }

    func handleAssetDetails(value: Data?, blockHash _: Data?) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        // we don't want to process asset details change transactions
        let processingBlockHash = receivedDetails ? nil : lastAccountValueHash

        receivedDetails = true
        lastDetailsValue = value

        checkChanges(
            chainAssetId: chainAssetId,
            accountId: accountId,
            blockHash: processingBlockHash,
            logger: logger
        )
    }

    func handleAssetAccount(value: Data?, blockHash: Data?) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        receivedAccount = true
        lastAccountValue = value
        lastAccountValueHash = blockHash

        checkChanges(chainAssetId: chainAssetId, accountId: accountId, blockHash: blockHash, logger: logger)
    }
}

private extension AssetsBalanceUpdater {
    func createAccountWrapper(
        for lastAccountValue: Data?
    ) -> CompoundOperationWrapper<AssetsPallet.Account?> {
        let assetAccountPath = AssetsPallet.accountPath(from: extras.palletName)
        return createStorageDecoderWrapper(for: lastAccountValue, path: assetAccountPath)
    }

    func createDetailsWrapper(
        for lastDetailsValue: Data?
    ) -> CompoundOperationWrapper<AssetsPallet.Details?> {
        let assetDetailsPath = AssetsPallet.detailsPath(from: extras.palletName)
        return createStorageDecoderWrapper(for: lastDetailsValue, path: assetDetailsPath)
    }

    func createStorageDecoderWrapper<T: Decodable>(
        for value: Data?,
        path: StorageCodingPath
    ) -> CompoundOperationWrapper<T?> {
        guard let storageData = value else {
            return CompoundOperationWrapper.createWithResult(nil)
        }

        guard let runtimeProvider = chainRegistry.getRuntimeProvider(for: chainAssetId.chainId) else {
            return CompoundOperationWrapper.createWithError(ChainRegistryError.runtimeMetadaUnavailable)
        }

        let codingFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()

        let decodingOperation = StorageDecodingOperation<T>(path: path, data: storageData)
        decodingOperation.configurationBlock = {
            do {
                decodingOperation.codingFactory = try codingFactoryOperation
                    .extractNoCancellableResultData()
            } catch {
                decodingOperation.result = .failure(error)
            }
        }

        decodingOperation.addDependency(codingFactoryOperation)

        let mappingOperation = ClosureOperation<T?> {
            try decodingOperation.extractNoCancellableResultData()
        }

        mappingOperation.addDependency(decodingOperation)

        return CompoundOperationWrapper(
            targetOperation: mappingOperation,
            dependencies: [codingFactoryOperation, decodingOperation]
        )
    }

    func createChangesOperation(
        dependingOn detailsWrapper: CompoundOperationWrapper<AssetsPallet.Details?>,
        accountWrapper: CompoundOperationWrapper<AssetsPallet.Account?>,
        chainAssetId: ChainAssetId,
        accountId: AccountId
    ) -> BaseOperation<AssetBalance> {
        ClosureOperation<AssetBalance> {
            let assetAccount = try accountWrapper.targetOperation.extractNoCancellableResultData()

            let balance = assetAccount?.balance ?? 0

            let assetDetails = try detailsWrapper.targetOperation.extractNoCancellableResultData()

            let isFrozen = (assetAccount?.isFrozen ?? false) || (assetDetails?.isFrozen ?? false)
            let isBlocked = assetAccount?.isBlocked ?? false

            return AssetBalance(
                chainAssetId: chainAssetId,
                accountId: accountId,
                freeInPlank: balance,
                reservedInPlank: 0,
                frozenInPlank: isFrozen ? balance : 0,
                edCountMode: .basedOnTotal,
                transferrableMode: .regular,
                blocked: isBlocked
            )
        }
    }

    func checkChanges(
        chainAssetId: ChainAssetId,
        accountId: AccountId,
        blockHash: Data?,
        logger: LoggerProtocol
    ) {
        if receivedAccount, receivedDetails {
            let assetAccountWrapper = createAccountWrapper(for: lastAccountValue)
            let assetDetailsWrapper = createDetailsWrapper(for: lastDetailsValue)

            let changesOperation = createChangesOperation(
                dependingOn: assetDetailsWrapper,
                accountWrapper: assetAccountWrapper,
                chainAssetId: chainAssetId,
                accountId: accountId
            )

            let saveOperation = ClosureOperation<Void> { [weak self] in
                let assetBalance = try changesOperation.extractNoCancellableResultData()
                self?.balanceUpdateProcessor.process(
                    balance: assetBalance,
                    blockHash: blockHash
                )
            }

            changesOperation.addDependency(assetAccountWrapper.targetOperation)
            changesOperation.addDependency(assetDetailsWrapper.targetOperation)
            saveOperation.addDependency(changesOperation)

            let dependencies = assetAccountWrapper.allOperations + assetDetailsWrapper
                .allOperations + [changesOperation]

            let wrapper = CompoundOperationWrapper(
                targetOperation: saveOperation,
                dependencies: dependencies
            )

            logger.debug("Processing changes for assets: \(chainAssetId)")

            operationQueue.addOperations(wrapper.allOperations, waitUntilFinished: false)
        }
    }
}
