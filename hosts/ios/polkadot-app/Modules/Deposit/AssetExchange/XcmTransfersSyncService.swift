import Foundation
import XcmTransfer
import Operation_iOS
import CommonService

protocol XcmTransfersSyncServiceProtocol: AnyObject, ApplicationServiceProtocol {
    var notificationCallback: ((Result<XcmTransfers, Error>) -> Void)? { get set }
    var notificationQueue: DispatchQueue { get set }
}

final class XcmTransfersSyncService {
    var notificationCallback: ((Result<XcmTransfers, Error>) -> Void)?
    var notificationQueue: DispatchQueue = .main

    let remoteConfigManager: RemoteConfigManaging
    let operationQueue: OperationQueue
    let logger: LoggerProtocol
    let workQueue: DispatchQueue

    private var cancellableStore = CancellableCallStore()

    init(
        remoteConfigManager: RemoteConfigManaging,
        operationQueue: OperationQueue,
        workQueue: DispatchQueue = DispatchQueue(label: "io.xcmsync.service"),
        logger: LoggerProtocol
    ) {
        self.remoteConfigManager = remoteConfigManager
        self.operationQueue = operationQueue
        self.workQueue = workQueue
        self.logger = logger
    }
}

private extension XcmTransfersSyncService {
    func fetchAndUpdateTransfers() {
        logger.debug("Fetching xcm config")

        let transfersConfigWrapper: CompoundOperationWrapper<XcmDynamicTransfers>
        transfersConfigWrapper = remoteConfigManager.asyncWaitXcmTransfers()

        let generalConfigWrapper: CompoundOperationWrapper<XcmGeneralConfig>
        generalConfigWrapper = remoteConfigManager.asyncWaitXcmGeneralConfig()

        let mappingOperation = ClosureOperation<(XcmDynamicTransfers, XcmGeneralConfig)> {
            let transfersConfig = try transfersConfigWrapper.targetOperation.extractNoCancellableResultData()
            let generalConfig = try generalConfigWrapper.targetOperation.extractNoCancellableResultData()
            return (transfersConfig, generalConfig)
        }

        mappingOperation.addDependency(transfersConfigWrapper.targetOperation)
        mappingOperation.addDependency(generalConfigWrapper.targetOperation)

        let totalWrapper = generalConfigWrapper
            .insertingHead(operations: transfersConfigWrapper.allOperations)
            .insertingTail(operation: mappingOperation)

        executeCancellable(
            wrapper: totalWrapper,
            inOperationQueue: operationQueue,
            backingCallIn: cancellableStore,
            runningCallbackIn: notificationQueue
        ) { [weak self] result in
            switch result {
            case let .success(config):
                self?.logger.debug("Config fetched sucessfully!")

                let xcmTransfers = XcmTransfers(
                    legacyTransfers: XcmLegacyTransfers.empty,
                    dynamicTransfers: config.0,
                    generalConfig: config.1
                )

                self?.notificationCallback?(.success(xcmTransfers))
            case let .failure(error):
                self?.logger.error("Config fetch error: \(error)")
                self?.notificationCallback?(.failure(error))
            }
        }
    }
}

extension XcmTransfersSyncService: XcmTransfersSyncServiceProtocol {
    func setup() {
        workQueue.async { [weak self] in
            self?.fetchAndUpdateTransfers()
        }
    }

    func throttle() {
        workQueue.async { [weak self] in
            self?.cancellableStore.cancel()
        }
    }
}
