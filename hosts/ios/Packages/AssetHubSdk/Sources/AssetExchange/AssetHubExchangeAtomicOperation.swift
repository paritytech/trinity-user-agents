import Foundation
import Operation_iOS
import ExtrinsicService
import SubstrateSdk
import AssetExchange

enum AssetHubExchangeAtomicOperationError: Error {
    case noEventsInResult
}

final class AssetHubExchangeAtomicOperation {
    let host: AssetHubExchangeHostProtocol
    let edge: any AssetExchangableGraphEdge
    let operationArgs: AssetExchangeAtomicOperationArgs

    init(
        host: AssetHubExchangeHostProtocol,
        operationArgs: AssetExchangeAtomicOperationArgs,
        edge: any AssetExchangableGraphEdge
    ) {
        self.host = host
        self.operationArgs = operationArgs
        self.edge = edge
    }

    private func createFeeWrapper(creditingTo accountId: AccountId?) -> CompoundOperationWrapper<ExtrinsicFeeProtocol> {
        let receiver = accountId ?? host.selectedAccount.accountId

        let callArgs = AssetConversion.CallArgs(
            assetIn: edge.origin,
            amountIn: operationArgs.swapLimit.amountIn,
            assetOut: edge.destination,
            amountOut: operationArgs.swapLimit.amountOut,
            receiver: receiver,
            direction: operationArgs.swapLimit.direction,
            slippage: operationArgs.swapLimit.slippage
        )

        let codingFactoryOperation = host.runtimeService.fetchCoderFactoryOperation()

        let builderClosure: ExtrinsicBuilderClosure = { builder in
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()

            return try self.host.extrinsicConverting.addingOperation(
                to: builder,
                chain: self.host.chain,
                args: callArgs,
                codingFactory: codingFactory
            )
        }

        let feeWrapper = host.extrinsicOperationFactory.estimateFeeOperation(
            builderClosure,
            origin: host.originDefiner,
            payingIn: operationArgs.feeAsset
        )

        feeWrapper.addDependency(operations: [codingFactoryOperation])

        return feeWrapper.insertingHead(operations: [codingFactoryOperation])
    }
}

extension AssetHubExchangeAtomicOperation: AssetExchangeAtomicOperationProtocol {
    func executeWrapper(
        for swapLimit: AssetExchangeSwapLimit,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<Balance> {
        let codingFactoryOperation = host.runtimeService.fetchCoderFactoryOperation()

        let receiver = accountId ?? host.selectedAccount.accountId

        let executeWrapper = OperationCombiningService<Balance>.compoundNonOptionalWrapper(
            operationQueue: host.operationQueue
        ) {
            let callArgs = AssetConversion.CallArgs(
                assetIn: self.edge.origin,
                amountIn: swapLimit.amountIn,
                assetOut: self.edge.destination,
                amountOut: swapLimit.amountOut,
                receiver: receiver,
                direction: swapLimit.direction,
                slippage: swapLimit.slippage
            )

            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()

            let builderClosure: ExtrinsicBuilderClosure = { builder in
                try self.host.extrinsicConverting.addingOperation(
                    to: builder,
                    chain: self.host.chain,
                    args: callArgs,
                    codingFactory: codingFactory
                )
            }

            let submittionWrapper = self.host.submissionMonitorFactory.submitAndMonitorWrapper(
                extrinsicBuilderClosure: builderClosure,
                origin: self.host.originDefiner,
                params: ExtrinsicSubmissionParams(
                    feeAssetId: self.operationArgs.feeAsset,
                    eventsMatcher: AssetConversionEventsMatching()
                )
            )

            let codingFactoryOperation = self.host.runtimeService.fetchCoderFactoryOperation()

            let monitorOperation = ClosureOperation<Balance> {
                let submittionResult = try submittionWrapper.targetOperation.extractNoCancellableResultData()
                let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()

                switch submittionResult.status {
                case let .success(executionResult):
                    let eventParser = AssetConversionEventParser(logger: self.host.logger)

                    self.host.logger.debug("Execution success: \(executionResult.interestedEvents)")

                    guard let amountOut = eventParser.extractDeposit(
                        from: executionResult.interestedEvents,
                        using: codingFactory
                    ) else {
                        throw AssetHubExchangeAtomicOperationError.noEventsInResult
                    }

                    self.host.logger.debug("Arrived amount: \(String(amountOut))")

                    return amountOut
                case let .failure(executionFailure):
                    throw executionFailure.error
                }
            }

            monitorOperation.addDependency(submittionWrapper.targetOperation)
            monitorOperation.addDependency(codingFactoryOperation)

            return submittionWrapper
                .insertingHead(operations: [codingFactoryOperation])
                .insertingTail(operation: monitorOperation)
        }

        executeWrapper.addDependency(operations: [codingFactoryOperation])

        return executeWrapper.insertingHead(operations: [codingFactoryOperation])
    }

    func submitWrapper(
        for swapLimit: AssetExchangeSwapLimit,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel> {
        let receiver = accountId ?? host.selectedAccount.accountId
        let codingFactoryOperation = host.runtimeService.fetchCoderFactoryOperation()

        let callArgs = AssetConversion.CallArgs(
            assetIn: edge.origin,
            amountIn: swapLimit.amountIn,
            assetOut: edge.destination,
            amountOut: swapLimit.amountOut,
            receiver: receiver,
            direction: swapLimit.direction,
            slippage: swapLimit.slippage
        )

        let builderClosure: ExtrinsicBuilderClosure = { builder in
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
            return try self.host.extrinsicConverting.addingOperation(
                to: builder,
                chain: self.host.chain,
                args: callArgs,
                codingFactory: codingFactory
            )
        }

        let submittionWrapper = host.submissionMonitorFactory.submitAndMonitorWrapper(
            extrinsicBuilderClosure: builderClosure,
            origin: host.originDefiner,
            params: ExtrinsicSubmissionParams(
                feeAssetId: operationArgs.feeAsset,
                eventsMatcher: nil
            )
        )

        submittionWrapper.addDependency(operations: [codingFactoryOperation])

        let mappingOperation = ClosureOperation<ExtrinsicSubmittedModel> {
            let model = try submittionWrapper.targetOperation.extractNoCancellableResultData()
            return model.extrinsicSubmittedModel
        }

        mappingOperation.addDependency(submittionWrapper.targetOperation)

        return submittionWrapper
            .insertingHead(operations: [codingFactoryOperation])
            .insertingTail(operation: mappingOperation)
    }

    func estimateFee(creditingTo accountId: AccountId?) -> CompoundOperationWrapper<AssetExchangeOperationFee> {
        let feeWrapper = createFeeWrapper(creditingTo: accountId)

        let mappingOperation = ClosureOperation<AssetExchangeOperationFee> {
            let extrinsicFee = try feeWrapper.targetOperation.extractNoCancellableResultData()

            return AssetExchangeOperationFee(
                extrinsicFee: extrinsicFee,
                args: self.operationArgs,
                postTransfer: .free
            )
        }

        mappingOperation.addDependency(feeWrapper.targetOperation)

        return feeWrapper.insertingTail(operation: mappingOperation)
    }

    func requiredAmountToGetAmountOut(
        _ amountOutClosure: @escaping () throws -> Balance
    ) -> CompoundOperationWrapper<Balance> {
        OperationCombiningService.compoundNonOptionalWrapper(
            operationQueue: host.operationQueue
        ) {
            let amountOut = try amountOutClosure()

            return self.edge.quote(amount: amountOut, direction: .buy)
        }
    }

    var swapLimit: AssetExchangeSwapLimit {
        operationArgs.swapLimit
    }
}
