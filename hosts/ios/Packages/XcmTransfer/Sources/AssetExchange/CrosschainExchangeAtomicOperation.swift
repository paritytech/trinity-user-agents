import Foundation
import Operation_iOS
import ExtrinsicService
import SubstrateSdk
import AssetExchange

enum CrosschainExchangeAtomicOperationError: Error {
    case missingChain(ChainId)
}

final class CrosschainExchangeAtomicOperation {
    let host: CrosschainExchangeHostProtocol
    let operationArgs: AssetExchangeAtomicOperationArgs
    let edge: any AssetExchangableGraphEdge
    let workingQueue: DispatchQueue

    init(
        host: CrosschainExchangeHostProtocol,
        edge: any AssetExchangableGraphEdge,
        operationArgs: AssetExchangeAtomicOperationArgs,
        workingQueue: DispatchQueue = .global()
    ) {
        self.host = host
        self.edge = edge
        self.operationArgs = operationArgs
        self.workingQueue = workingQueue
    }

    private func createXcmPartiesResolutionWrapper(
        for destinationAccountId: AccountId
    ) -> CompoundOperationWrapper<XcmTransferParties> {
        host.resolutionFactory.createResolutionWrapper(
            for: edge.origin,
            transferDestinationId: .init(
                chainAssetId: edge.destination,
                accountId: destinationAccountId
            ),
            xcmTransfers: host.xcmTransfers
        )
    }

    private func createOriginFeeFetchWrapper(
        dependingOn resolutionOperation: BaseOperation<XcmTransferParties>,
        amountClosure: @escaping () throws -> Balance
    ) -> CompoundOperationWrapper<ExtrinsicFeeProtocol> {
        OperationCombiningService<ExtrinsicFeeProtocol>.compoundNonOptionalWrapper(
            operationQueue: host.operationQueue
        ) {
            let transferParties = try resolutionOperation.extractNoCancellableResultData()
            let amount = try amountClosure()

            let unweightedRequest = XcmUnweightedTransferRequest(
                origin: transferParties.origin,
                destination: transferParties.destination,
                reserve: transferParties.reserve,
                metadata: transferParties.metadata,
                amount: amount
            )

            let transferRequest = XcmTransferRequest(
                unweighted: unweightedRequest,
                originFeeAsset: self.operationArgs.feeAsset
            )

            let feeOperation = AsyncClosureOperation<ExtrinsicFeeProtocol> { completion in
                self.host.xcmService.estimateOriginFee(
                    request: transferRequest,
                    runningIn: self.workingQueue
                ) { result in
                    completion(result)
                }
            }

            return CompoundOperationWrapper(targetOperation: feeOperation)
        }
    }

    private func createCrosschainFeeFetchWrapper(
        dependingOn resolutionOperation: BaseOperation<XcmTransferParties>,
        amountClosure: @escaping () throws -> Balance
    ) -> CompoundOperationWrapper<XcmFeeModelProtocol> {
        OperationCombiningService<XcmFeeModelProtocol>.compoundNonOptionalWrapper(
            operationQueue: host.operationQueue
        ) {
            let transferParties = try resolutionOperation.extractNoCancellableResultData()
            let amount = try amountClosure()

            let request = XcmUnweightedTransferRequest(
                origin: transferParties.origin,
                destination: transferParties.destination,
                reserve: transferParties.reserve,
                metadata: transferParties.metadata,
                amount: amount
            )

            let feeOperation = AsyncClosureOperation<XcmFeeModelProtocol> { completion in
                self.host.xcmService.estimateCrossChainFee(
                    request: request,
                    runningIn: self.workingQueue
                ) { result in
                    completion(result)
                }
            }

            return CompoundOperationWrapper(targetOperation: feeOperation)
        }
    }

    private func createSubmitAndWaitArrivalWrapper(
        destinationAsset: ChainAssetProtocol,
        resolutionOperation: BaseOperation<XcmTransferParties>,
        amountClosure: @escaping () throws -> Balance
    ) -> CompoundOperationWrapper<Balance> {
        OperationCombiningService<Balance>.compoundNonOptionalWrapper(
            operationQueue: host.operationQueue
        ) {
            let transferParties = try resolutionOperation.extractNoCancellableResultData()
            let amount = try amountClosure()

            let unweightedRequest = XcmUnweightedTransferRequest(
                origin: transferParties.origin,
                destination: transferParties.destination,
                reserve: transferParties.reserve,
                metadata: transferParties.metadata,
                amount: amount
            )

            let transferRequest = XcmTransferRequest(
                unweighted: unweightedRequest,
                originFeeAsset: self.operationArgs.feeAsset
            )

            let transactService = XcmTransactService(
                chainRegistry: self.host.chainRegistry,
                transferService: self.host.xcmService,
                tokensDepositMatchingFactory: self.host.tokensDepositMatchingFactory,
                balanceDetectionFactory: self.host.balanceDetectionFactory,
                workingQueue: self.workingQueue,
                operationQueue: self.host.operationQueue,
                logger: self.host.logger
            )

            return transactService.transferAndWaitArrivalWrapper(
                transferRequest,
                destinationChainAsset: destinationAsset
            )
        }
    }

    private func createSubmitTransferWrapper(
        resolutionOperation: BaseOperation<XcmTransferParties>,
        amountClosure: @escaping () throws -> Balance
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel> {
        OperationCombiningService<ExtrinsicSubmittedModel>.compoundNonOptionalWrapper(
            operationQueue: host.operationQueue
        ) {
            let transferParties = try resolutionOperation.extractNoCancellableResultData()
            let amount = try amountClosure()

            let unweightedRequest = XcmUnweightedTransferRequest(
                origin: transferParties.origin,
                destination: transferParties.destination,
                reserve: transferParties.reserve,
                metadata: transferParties.metadata,
                amount: amount
            )

            let transferRequest = XcmTransferRequest(
                unweighted: unweightedRequest,
                originFeeAsset: self.operationArgs.feeAsset
            )

            let transactService = XcmTransactService(
                chainRegistry: self.host.chainRegistry,
                transferService: self.host.xcmService,
                tokensDepositMatchingFactory: self.host.tokensDepositMatchingFactory,
                balanceDetectionFactory: self.host.balanceDetectionFactory,
                workingQueue: self.workingQueue,
                operationQueue: self.host.operationQueue,
                logger: self.host.logger
            )

            return transactService.submitTransferWrapper(transferRequest)
        }
    }
}

extension CrosschainExchangeAtomicOperation: AssetExchangeAtomicOperationProtocol {
    func executeWrapper(
        for swapLimit: AssetExchangeSwapLimit,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<Balance> {
        do {
            guard
                let destinationChain = host.allChains[edge.destination.chainId],
                let destinationAsset = destinationChain.chainAssetInterface(
                    for: edge.destination.assetId
                ) else {
                return .createWithError(
                    CrosschainExchangeAtomicOperationError.missingChain(
                        edge.destination.chainId
                    )
                )
            }

            let receiver = try accountId ?? host.wallet.fetchAccount(for: destinationChain).accountId

            let resolutionWrapper = createXcmPartiesResolutionWrapper(for: receiver)

            let submitWrapper = createSubmitAndWaitArrivalWrapper(
                destinationAsset: destinationAsset,
                resolutionOperation: resolutionWrapper.targetOperation,
                amountClosure: { swapLimit.amountIn }
            )

            submitWrapper.addDependency(wrapper: resolutionWrapper)

            return submitWrapper.insertingHead(operations: resolutionWrapper.allOperations)
        } catch {
            return .createWithError(error)
        }
    }

    func submitWrapper(
        for swapLimit: AssetExchangeSwapLimit,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel> {
        do {
            guard
                let destinationChain = host.allChains[edge.destination.chainId] else {
                return .createWithError(
                    CrosschainExchangeAtomicOperationError.missingChain(
                        edge.destination.chainId
                    )
                )
            }

            let receiver = try accountId ?? host.wallet.fetchAccount(for: destinationChain).accountId

            let resolutionWrapper = createXcmPartiesResolutionWrapper(for: receiver)

            let submitWrapper = createSubmitTransferWrapper(
                resolutionOperation: resolutionWrapper.targetOperation,
                amountClosure: { swapLimit.amountIn }
            )

            submitWrapper.addDependency(wrapper: resolutionWrapper)

            return submitWrapper.insertingHead(operations: resolutionWrapper.allOperations)
        } catch {
            return .createWithError(error)
        }
    }

    func estimateFee(creditingTo accountId: AccountId?) -> CompoundOperationWrapper<AssetExchangeOperationFee> {
        do {
            guard
                let originChain = host.allChains[edge.origin.chainId],
                let originUtilityAsset = originChain.utilityChainAssetInterface() else {
                return .createWithError(
                    CrosschainExchangeAtomicOperationError.missingChain(
                        edge.origin.chainId
                    )
                )
            }

            guard let destinationChain = host.allChains[edge.destination.chainId] else {
                return .createWithError(
                    CrosschainExchangeAtomicOperationError.missingChain(
                        edge.destination.chainId
                    )
                )
            }

            let receiver = try accountId ?? host.wallet.fetchAccount(for: destinationChain).accountId

            let resolutionWrapper = createXcmPartiesResolutionWrapper(for: receiver)

            let originFeeWrapper = createOriginFeeFetchWrapper(
                dependingOn: resolutionWrapper.targetOperation,
                amountClosure: { self.operationArgs.swapLimit.amountIn }
            )

            originFeeWrapper.addDependency(wrapper: resolutionWrapper)

            let crosschainFeeWrapper = createCrosschainFeeFetchWrapper(
                dependingOn: resolutionWrapper.targetOperation,
                amountClosure: { self.operationArgs.swapLimit.amountIn }
            )

            crosschainFeeWrapper.addDependency(wrapper: resolutionWrapper)

            let mappingOperation = ClosureOperation<AssetExchangeOperationFee> {
                let originFee = try originFeeWrapper.targetOperation.extractNoCancellableResultData()
                let crosschainFee = try crosschainFeeWrapper.targetOperation.extractNoCancellableResultData()

                return .init(
                    crosschainFee: crosschainFee,
                    originFee: originFee,
                    assetIn: self.edge.origin,
                    assetOut: self.edge.destination,
                    originUtilityAsset: originUtilityAsset.chainAssetId,
                    args: self.operationArgs
                )
            }

            mappingOperation.addDependency(crosschainFeeWrapper.targetOperation)
            mappingOperation.addDependency(originFeeWrapper.targetOperation)

            return originFeeWrapper
                .insertingHead(operations: crosschainFeeWrapper.allOperations)
                .insertingHead(operations: resolutionWrapper.allOperations)
                .insertingTail(operation: mappingOperation)
        } catch {
            return .createWithError(error)
        }
    }

    func requiredAmountToGetAmountOut(
        _ amountOutClosure: @escaping () throws -> Balance
    ) -> CompoundOperationWrapper<Balance> {
        let operation = ClosureOperation {
            try amountOutClosure()
        }

        return CompoundOperationWrapper(targetOperation: operation)
    }

    var swapLimit: AssetExchangeSwapLimit {
        operationArgs.swapLimit
    }
}
