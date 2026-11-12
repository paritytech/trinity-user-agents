import Foundation
import Operation_iOS
import SubstrateSdk
import CommonService
import ChainRegistry
import EventCenter

final class ChainSyncService: BaseSyncService {
    struct SyncModel {
        let newOrUpdated: [ChainModel]
        let removed: [ChainModel]
    }

    // MARK: Properties

    private unowned let remoteConfigManager: RemoteConfigManaging
    private let chainConverter: ChainModelConversionProtocol
    private let repository: AnyDataProviderRepository<ChainModel>
    private let eventCenter: EventCenterProtocol
    private let operationQueue: OperationQueue

    // MARK: Initial methods

    init(
        remoteConfigManager: RemoteConfigManaging,
        chainConverter: ChainModelConversionProtocol,
        repository: AnyDataProviderRepository<ChainModel>,
        eventCenter: EventCenterProtocol,
        operationQueue: OperationQueue,
        retryStrategy: ReconnectionStrategyProtocol = ExponentialReconnection(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.remoteConfigManager = remoteConfigManager
        self.chainConverter = chainConverter
        self.repository = repository
        self.eventCenter = eventCenter
        self.operationQueue = operationQueue

        super.init(retryStrategy: retryStrategy, logger: logger)
    }

    override func performSyncUp() {
        logger.debug("[ChainSync] Did start chains sync")

        let localChainsOperation = repository.fetchAllOperation(with: .init())
        let remoteChainsWrapper = remoteConfigManager.asyncWaitChainsForRemoteConfigValues()

        let mappingOperation = createMappingOperation(
            localChainsOperation: localChainsOperation,
            remoteChainsWrapper: remoteChainsWrapper
        )
        mappingOperation.addDependency(remoteChainsWrapper.targetOperation)
        mappingOperation.addDependency(localChainsOperation)

        let saveOperation = createSaveOperation(mappingOperation: mappingOperation)
        saveOperation.addDependency(mappingOperation)

        let totalWrapper = remoteChainsWrapper
            .insertingHead(operations: [localChainsOperation])
            .insertingTail(operation: mappingOperation)
            .insertingTail(operation: saveOperation)

        execute(
            wrapper: totalWrapper,
            inOperationQueue: operationQueue,
            runningCallbackIn: nil
        ) { [weak self] result in
            switch result {
            case .success:
                try? self?.complete(result: .success(mappingOperation.extractNoCancellableResultData()))
            case let .failure(error):
                self?.complete(result: .failure(error))
            }
        }
    }

    // MARK: Private methods

    private func createMappingOperation(
        localChainsOperation: BaseOperation<[ChainModel]>,
        remoteChainsWrapper: CompoundOperationWrapper<[RemoteChainModel]>
    ) -> ClosureOperation<SyncModel> {
        ClosureOperation<SyncModel> {
            let localModels = try localChainsOperation.extractNoCancellableResultData()
            do {
                let remoteModels = try remoteChainsWrapper.targetOperation.extractNoCancellableResultData()

                self.logger.debug("[ChainSync] remote chains found \(remoteModels.count)")

                remoteModels.forEach { model in
                    self.logger.debug(
                        "[ChainSync] remote chain id=\(model.chainId) name=\(model.name) nodes=\(model.nodes.map(\.url))"
                    )
                }

                let newOrUpdated = remoteModels.enumerated().compactMap { index, object in
                    self.chainConverter.update(
                        localModel: localModels.first(where: { $0.chainId == object.chainId }),
                        remoteModel: object,
                        additionalAssets: [],
                        order: Int64(index)
                    )
                }

                let remoteIds = Set(remoteModels.map(\.chainId))
                let chainsToRemove = localModels
                    .filter { !remoteIds.contains($0.chainId) }

                return SyncModel(newOrUpdated: newOrUpdated, removed: chainsToRemove)
            } catch {
                return SyncModel(newOrUpdated: localModels, removed: [])
            }
        }
    }

    private func createSaveOperation(
        mappingOperation: ClosureOperation<SyncModel>
    ) -> BaseOperation<Void> {
        repository.saveOperation {
            try mappingOperation.extractNoCancellableResultData().newOrUpdated
        } _: { try mappingOperation.extractNoCancellableResultData().removed.map(\.chainId) }
    }

    private func complete(result: Result<SyncModel, Error>) {
        switch result {
        case let .success(syncModel):
            logger.debug("Sync newOrUpdated: \(syncModel.newOrUpdated.count)")
            logger.debug("Sync removed: \(syncModel.removed.count)")

            let event = ChainSyncDidComplete(
                newOrUpdatedChains: syncModel.newOrUpdated,
                removedChains: syncModel.removed
            )

            eventCenter.notify(with: event)

            complete(nil)
        case let .failure(error):
            logger.error("Sync failed with error: \(error)")

            let event = ChainSyncDidFail(error: error)
            eventCenter.notify(with: event)

            complete(error)
        }
    }
}

// MARK: - ChainSyncServiceProtocol

extension ChainSyncService: ChainSyncServiceProtocol {
    func syncUpChains() {
        if getIsActive() {
            syncUp()
        } else {
            setup()
        }
    }

    func updateLocal(chain: ChainModel) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let operation = repository.saveOperation({ [chain] }, { [] })

        operationQueue.addOperation(operation)
    }
}
