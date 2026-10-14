import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateSdkExt

public extension RuntimeSyncService {
    func performSync(
        for chainId: ChainModel.Id,
        shouldSyncTypes: Bool,
        newVersion: RuntimeVersion? = nil
    ) {
        guard let syncInfo = knownChains[chainId] else {
            return
        }

        let chainTypesSyncWrapper = shouldSyncTypes ? syncInfo.typesURL.map {
            createChainTypesSyncOperation(chainId, hasher: dataHasher, url: $0)
        } : nil

        let metadataSyncWrapper = newVersion.map {
            createMetadataSyncOperation(
                for: chainId,
                runtimeVersion: $0,
                connection: syncInfo.connection,
                runtimeFetchFactory: runtimeFetchFactory,
                runtimeLocalMigrator: runtimeLocalMigrator
            )
        }

        if chainTypesSyncWrapper == nil, metadataSyncWrapper == nil {
            return
        }

        let dependencies = (chainTypesSyncWrapper?.allOperations ?? []) + (metadataSyncWrapper?.allOperations ?? [])

        let processingOperation = ClosureOperation<SyncResult> {
            SyncResult(
                chainId: chainId,
                typesSyncResult: chainTypesSyncWrapper?.targetOperation.result,
                metadataSyncResult: metadataSyncWrapper?.targetOperation.result,
                runtimeVersion: newVersion
            )
        }

        dependencies.forEach { processingOperation.addDependency($0) }

        processingOperation.completionBlock = { [weak self] in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    let result = try processingOperation.extractNoCancellableResultData()
                    self?.processSyncResult(result)
                } catch let error as BaseOperationError where error == .parentOperationCancelled {
                    return
                } catch {
                    let result = SyncResult(
                        chainId: chainId,
                        typesSyncResult: .failure(error),
                        metadataSyncResult: .failure(error),
                        runtimeVersion: newVersion
                    )

                    self?.logger?.error("Error: \(error)")

                    self?.processSyncResult(result)
                }
            }
        }

        let wrapper = CompoundOperationWrapper(targetOperation: processingOperation, dependencies: dependencies)

        syncingChains[chainId] = wrapper

        operationQueue.addOperations(wrapper.allOperations, waitUntilFinished: false)
    }

    private func processSyncResult(_ result: SyncResult) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        syncingChains[result.chainId] = nil

        addRetryRequestIfNeeded(for: result)

        notifyCompletion(for: result)
    }

    private func addRetryRequestIfNeeded(for result: SyncResult) {
        let shouldSyncTypes =
            if case .failure = result.typesSyncResult {
                true
            } else {
                false
            }

        let runtimeSyncVersion: RuntimeVersion? =
            if let version = result.runtimeVersion,
            case .failure = result.metadataSyncResult {
                version
            } else {
                nil
            }

        if shouldSyncTypes || (runtimeSyncVersion != nil) {
            let nextAttempt = retryAttempts[result.chainId].map { $0.attempt + 1 } ?? 1

            let retryAttempt = RetryAttempt(
                chainId: result.chainId,
                shouldSyncTypes: shouldSyncTypes,
                runtimeVersion: runtimeSyncVersion,
                attempt: nextAttempt
            )

            retryAttempts[result.chainId] = retryAttempt

            rescheduleRetryIfNeeded()
        } else {
            retryAttempts[result.chainId] = nil
        }
    }

    private func rescheduleRetryIfNeeded() {
        guard retryScheduler == nil else {
            return
        }

        guard let maxAttempt = retryAttempts.max(by: { $0.value.attempt < $1.value.attempt })?
            .value.attempt else {
            return
        }

        if let delay = retryStrategy.reconnectAfter(attempt: maxAttempt) {
            retryScheduler = Scheduler(with: self)
            retryScheduler?.notifyAfter(delay)
        }
    }

    private func notifyCompletion(for result: SyncResult) {
        logger?.debug("Did complete sync \(result)")

        if case let .success(fileHash) = result.typesSyncResult {
            logger?.debug("Did sync chain type: \(result.chainId)")

            let event = RuntimeChainTypesSyncCompleted(chainId: result.chainId, fileHash: fileHash)
            eventCenter.notify(with: event)
        }

        if case .success = result.metadataSyncResult, let version = result.runtimeVersion {
            logger?.debug("Did sync metadata: \(result.chainId)")

            let event = RuntimeMetadataSyncCompleted(chainId: result.chainId, version: version)
            eventCenter.notify(with: event)
        }
    }

    private func createChainTypesSyncOperation(
        _ chainId: ChainModel.Id,
        hasher: StorageHasher,
        url: URL
    ) -> CompoundOperationWrapper<String> {
        let remoteFileOperation = dataOperationFactory.fetchData(from: url)

        let fileSaveWrapper = filesOperationFactory.saveChainTypesOperation(for: chainId) {
            try remoteFileOperation.extractNoCancellableResultData()
        }

        fileSaveWrapper.addDependency(operations: [remoteFileOperation])

        let mapOperation = ClosureOperation<String> {
            _ = try fileSaveWrapper.targetOperation.extractNoCancellableResultData()
            let data = try remoteFileOperation.extractNoCancellableResultData()

            return try hasher.hash(data: data).toHex()
        }

        mapOperation.addDependency(fileSaveWrapper.targetOperation)
        mapOperation.addDependency(remoteFileOperation)

        return CompoundOperationWrapper(
            targetOperation: mapOperation,
            dependencies: [remoteFileOperation] + fileSaveWrapper.allOperations
        )
    }

    private func createMetadataSyncOperation(
        for chainId: ChainModel.Id,
        runtimeVersion: RuntimeVersion,
        connection: JSONRPCEngine,
        runtimeFetchFactory: RuntimeFetchOperationFactoryProtocol,
        runtimeLocalMigrator: RuntimeLocalMigrating
    ) -> CompoundOperationWrapper<Void> {
        let localMetadataOperation = repository.fetchOperation(by: chainId, options: RepositoryFetchOptions())

        let remoteFetchWrapper: CompoundOperationWrapper<RawRuntimeMetadata>
        remoteFetchWrapper = OperationCombiningService.compoundNonOptionalWrapper(
            operationManager: OperationManager(operationQueue: operationQueue)
        ) {
            let currentItem = try localMetadataOperation
                .extractResultData(throwing: BaseOperationError.parentOperationCancelled)

            if
                let item = currentItem,
                item.version == runtimeVersion.specVersion,
                item.txVersion == runtimeVersion.transactionVersion,
                !runtimeLocalMigrator.needsMigration(for: item) {
                throw RuntimeSyncServiceError.skipMetadataUnchanged
            }

            return runtimeFetchFactory.createMetadataFetchWrapper(
                for: chainId,
                connection: connection
            )
        }

        remoteFetchWrapper.addDependency(operations: [localMetadataOperation])

        let saveMetadataOperation = repository.saveOperation({
            let rawMetadata = try remoteFetchWrapper.targetOperation.extractNoCancellableResultData()
            let metadataItem = RuntimeMetadataItem(
                chain: chainId,
                version: runtimeVersion.specVersion,
                txVersion: runtimeVersion.transactionVersion,
                localMigratorVersion: runtimeLocalMigrator.version,
                opaque: rawMetadata.isOpaque,
                metadata: rawMetadata.content
            )

            return [metadataItem]
        }, { [] })

        saveMetadataOperation.addDependency(remoteFetchWrapper.targetOperation)

        let filterOperation = ClosureOperation<Void> {
            do {
                _ = try saveMetadataOperation.extractNoCancellableResultData()
            } catch let error as RuntimeSyncServiceError where error == .skipMetadataUnchanged {
                return
            }
        }

        filterOperation.addDependency(saveMetadataOperation)

        let dependencies = [localMetadataOperation] + remoteFetchWrapper.allOperations + [saveMetadataOperation]

        return CompoundOperationWrapper(targetOperation: filterOperation, dependencies: dependencies)
    }
}
