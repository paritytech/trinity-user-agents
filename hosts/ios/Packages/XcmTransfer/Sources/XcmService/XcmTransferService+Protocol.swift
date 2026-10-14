import Foundation
import Operation_iOS
import SubstrateSdk
import ExtrinsicService
import BigInt

public typealias XcmTransferOriginFeeResult = Result<ExtrinsicFeeProtocol, Error>
public typealias XcmTransferOriginFeeClosure = (XcmTransferOriginFeeResult) -> Void

public typealias XcmTransferCrosschainFeeResult = Result<XcmFeeModelProtocol, Error>
public typealias XcmTransferCrosschainFeeClosure = (XcmTransferCrosschainFeeResult) -> Void

public struct XcmSubmitExtrinsic {
    let submittedModel: ExtrinsicSubmittedModel
    let callPath: CallCodingPath
}

public typealias XcmSubmitExtrinsicResult = Result<XcmSubmitExtrinsic, Error>
public typealias XcmExtrinsicSubmitClosure = (XcmSubmitExtrinsicResult) -> Void

public protocol XcmTransferServiceProtocol {
    func estimateOriginFee(
        request: XcmTransferRequest,
        runningIn queue: DispatchQueue,
        completion completionClosure: @escaping XcmTransferOriginFeeClosure
    )

    // Note: weight of the result contains max between reserve and destination weights
    func estimateCrossChainFee(
        request: XcmUnweightedTransferRequest,
        runningIn queue: DispatchQueue,
        completion completionClosure: @escaping XcmTransferCrosschainFeeClosure
    )

    func submit(
        request: XcmTransferRequest,
        runningIn queue: DispatchQueue,
        completion completionClosure: @escaping XcmExtrinsicSubmitClosure
    )
}

extension XcmTransferService: XcmTransferServiceProtocol {
    public func estimateOriginFee(
        request: XcmTransferRequest,
        runningIn queue: DispatchQueue,
        completion completionClosure: @escaping XcmTransferOriginFeeClosure
    ) {
        do {
            let unweighted = request.unweighted

            let callBuilderWrapper = callDerivator.createTransferCallDerivationWrapper(
                for: unweighted
            )

            let operationFactory = try extrinsicServiceFactory.createOperationFactory(
                chain: unweighted.originChain
            )

            let originDefiner = try originDefiningFactory.extrinsicOriginDefiner(
                from: wallet,
                chain: unweighted.originChain
            )

            let builder: ExtrinsicBuilderClosure = { builder in
                let collector = try callBuilderWrapper.targetOperation.extractNoCancellableResultData()
                return try collector.addingToExtrinsic(builder: builder)
            }

            let feeWrapper = operationFactory.estimateFeeOperation(
                builder,
                origin: originDefiner,
                payingIn: request.originFeeAsset
            )

            feeWrapper.addDependency(wrapper: callBuilderWrapper)

            let totalWrapper = feeWrapper.insertingHead(operations: callBuilderWrapper.allOperations)

            execute(
                wrapper: totalWrapper,
                inOperationQueue: operationQueue,
                runningCallbackIn: queue,
                callbackClosure: completionClosure
            )

        } catch {
            callbackClosureIfProvided(completionClosure, queue: queue, result: .failure(error))
        }
    }

    public func estimateCrossChainFee(
        request: XcmUnweightedTransferRequest,
        runningIn queue: DispatchQueue,
        completion completionClosure: @escaping XcmTransferCrosschainFeeClosure
    ) {
        let wrapper = crosschainFeeCalculator.crossChainFeeWrapper(
            request: request
        )

        execute(
            wrapper: wrapper,
            inOperationQueue: operationQueue,
            runningCallbackIn: queue,
            callbackClosure: completionClosure
        )
    }

    public func submit(
        request: XcmTransferRequest,
        runningIn queue: DispatchQueue,
        completion completionClosure: @escaping XcmExtrinsicSubmitClosure
    ) {
        do {
            let callBuilderWrapper = callDerivator.createTransferCallDerivationWrapper(
                for: request.unweighted
            )

            let chainAccount = try wallet.fetchAccount(for: request.unweighted.originChain)

            let operationFactory = try extrinsicServiceFactory.createOperationFactory(
                chain: request.unweighted.originChain
            )

            let originDefiner = try originDefiningFactory.extrinsicOriginDefiner(
                from: wallet,
                chain: request.unweighted.originChain
            )

            let verificationWrapper: CompoundOperationWrapper<Void> =
                if request.unweighted.metadata.isDynamicConfig {
                    submissionVerifier.createVerificationWrapper(
                        for: request.unweighted,
                        callOrigin: .system(.signed(chainAccount.accountId)), // TODO: origin must match provider
                        callClosure: {
                            try callBuilderWrapper.targetOperation.extractNoCancellableResultData()
                        }
                    )
                } else {
                    .createWithResult(())
                }

            verificationWrapper.addDependency(wrapper: callBuilderWrapper)

            let builderClosure: ExtrinsicBuilderClosure = { builder in
                // submit extrinsic only if verification passed
                try verificationWrapper.targetOperation.extractNoCancellableResultData()

                let collector = try callBuilderWrapper.targetOperation.extractNoCancellableResultData()
                return try collector.addingToExtrinsic(builder: builder)
            }

            let submitWrapper = operationFactory.submit(
                builderClosure,
                origin: originDefiner,
                payingIn: request.originFeeAsset
            )

            submitWrapper.addDependency(wrapper: callBuilderWrapper)
            submitWrapper.addDependency(wrapper: verificationWrapper)

            submitWrapper.targetOperation.completionBlock = {
                do {
                    let submittedModel = try submitWrapper.targetOperation.extractNoCancellableResultData()
                    let collector = try callBuilderWrapper.targetOperation.extractNoCancellableResultData()
                    let extrinsicResult = XcmSubmitExtrinsic(
                        submittedModel: submittedModel,
                        callPath: collector.callPath
                    )

                    callbackClosureIfProvided(completionClosure, queue: queue, result: .success(extrinsicResult))
                } catch {
                    callbackClosureIfProvided(completionClosure, queue: queue, result: .failure(error))
                }
            }

            let operations = callBuilderWrapper.allOperations + verificationWrapper.allOperations +
                submitWrapper.allOperations

            operationQueue.addOperations(operations, waitUntilFinished: false)
        } catch {
            callbackClosureIfProvided(completionClosure, queue: queue, result: .failure(error))
        }
    }
}
