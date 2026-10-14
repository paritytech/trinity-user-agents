import Foundation
import Operation_iOS
import ExtrinsicService
import SubstrateSdk
import ChainStore
import SDKLogger

public protocol XcmTransactServiceProtocol {
    func transferAndWaitArrivalWrapper(
        _ transferRequest: XcmTransferRequest,
        destinationChainAsset: ChainAssetProtocol
    ) -> CompoundOperationWrapper<Balance>

    func submitTransferWrapper(
        _ transferRequest: XcmTransferRequest
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel>
}

public final class XcmTransactService {
    let chainRegistry: ChainResourceProtocol
    let transferService: XcmTransferServiceProtocol
    let tokensDepositMatchingFactory: TokenDepositEventMatcherFactoryProtocol
    let balanceDetectionFactory: BalanceChangeDetectorFactoryProtocol
    let workingQueue: DispatchQueue
    let operationQueue: OperationQueue
    let logger: SDKLoggerProtocol

    public init(
        chainRegistry: ChainResourceProtocol,
        transferService: XcmTransferServiceProtocol,
        tokensDepositMatchingFactory: TokenDepositEventMatcherFactoryProtocol,
        balanceDetectionFactory: BalanceChangeDetectorFactoryProtocol,
        workingQueue: DispatchQueue,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.transferService = transferService
        self.tokensDepositMatchingFactory = tokensDepositMatchingFactory
        self.balanceDetectionFactory = balanceDetectionFactory
        self.workingQueue = workingQueue
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension XcmTransactService: XcmTransactServiceProtocol {
    public func transferAndWaitArrivalWrapper(
        _ transferRequest: XcmTransferRequest,
        destinationChainAsset: ChainAssetProtocol
    ) -> CompoundOperationWrapper<Balance> {
        let monitoringService = XcmDepositMonitoringService(
            accountId: transferRequest.unweighted.destination.accountId,
            chainAsset: destinationChainAsset,
            chainRegistry: chainRegistry,
            tokensDepositMatchingFactory: tokensDepositMatchingFactory,
            balanceDetectionFactory: balanceDetectionFactory,
            operationQueue: operationQueue,
            workingQueue: workingQueue,
            logger: logger
        )

        let monitoringWrapper = monitoringService.useMonitoringWrapper()

        let submittionOperation = AsyncClosureOperation<XcmSubmitExtrinsic> { completion in
            self.transferService.submit(
                request: transferRequest,
                runningIn: self.workingQueue
            ) { result in
                // cancel monitoring in case transaction submission failed
                if case .failure = result {
                    monitoringWrapper.cancel()
                }

                completion(result)
            }
        }

        let mappingOperation = ClosureOperation<Balance> {
            _ = try submittionOperation.extractNoCancellableResultData()

            let arrivedAmount = try monitoringWrapper.targetOperation.extractNoCancellableResultData()

            self.logger.debug("Arrived amount: \(String(arrivedAmount))")

            return arrivedAmount
        }

        mappingOperation.addDependency(monitoringWrapper.targetOperation)
        mappingOperation.addDependency(submittionOperation)

        let dependencies = monitoringWrapper.allOperations + [submittionOperation]

        return CompoundOperationWrapper(targetOperation: mappingOperation, dependencies: dependencies)
    }

    public func submitTransferWrapper(
        _ transferRequest: XcmTransferRequest
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel> {
        let submitOperation = AsyncClosureOperation<ExtrinsicSubmittedModel> { completion in
            self.transferService.submit(
                request: transferRequest,
                runningIn: self.workingQueue
            ) { result in
                switch result {
                case let .success(model):
                    completion(.success(model.submittedModel))
                case let .failure(error):
                    completion(.failure(error))
                }
            }
        }

        return CompoundOperationWrapper(targetOperation: submitOperation)
    }
}
