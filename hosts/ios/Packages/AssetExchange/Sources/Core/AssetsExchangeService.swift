import Foundation
import Operation_iOS
import SubstrateSdk
import ExtrinsicService
import CommonService
import SDKLogger

public protocol AssetsExchangeServiceProtocol: AnyObject, ApplicationServiceProtocol {
    func subscribeUpdates(
        for target: AnyObject,
        notifyingIn queue: DispatchQueue,
        closure: @escaping (AssetsExchangeGraphProviderStats) -> Void
    )

    func unsubscribeUpdates(for target: AnyObject)

    func fetchAssetsInWrapper(given assetOutId: ChainAssetId?) -> CompoundOperationWrapper<Set<ChainAssetId>>
    func fetchAssetsOutWrapper(given assetInId: ChainAssetId?) -> CompoundOperationWrapper<Set<ChainAssetId>>

    func fetchQuoteWrapper(for args: AssetConversion.QuoteArgs) -> CompoundOperationWrapper<AssetExchangeQuote>
    func estimateFee(for args: AssetExchangeFeeArgs) -> CompoundOperationWrapper<AssetExchangeFee>
    func canPayFee(in asset: ChainAssetProtocol) -> CompoundOperationWrapper<Bool>

    func submit(
        using estimation: AssetExchangeFee,
        creditingTo accountId: AccountId?,
        notifyingIn queue: DispatchQueue,
        operationStartClosure: @escaping (Int) -> Void
    ) -> CompoundOperationWrapper<Balance>

    func submitSingleOperationWrapper(
        using estimation: AssetExchangeFee,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel>

    func subscribeRequoteService(
        for target: AnyObject,
        ignoreIfAlreadyAdded: Bool,
        notifyingIn queue: DispatchQueue,
        closure: @escaping () -> Void
    )

    func throttleRequoteService()
}

public enum AssetsExchangeServiceError: Error {
    case noRoute
}

public final class AssetsExchangeService {
    let exchangesStateMediator: AssetsExchangeStateManaging
    let graphProvider: AssetsExchangeGraphProviding
    let feeSupportProvider: AssetsExchangeFeeSupportProviding
    let pathCostEstimator: AssetsExchangePathCostEstimating
    let operationQueue: OperationQueue
    let logger: SDKLoggerProtocol

    public init(
        graphProvider: AssetsExchangeGraphProviding,
        feeSupportProvider: AssetsExchangeFeeSupportProviding,
        exchangesStateMediator: AssetsExchangeStateManaging,
        pathCostEstimator: AssetsExchangePathCostEstimating,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.graphProvider = graphProvider
        self.feeSupportProvider = feeSupportProvider
        self.exchangesStateMediator = exchangesStateMediator
        self.pathCostEstimator = pathCostEstimator
        self.operationQueue = operationQueue
        self.logger = logger
    }

    private func prepareWrapper<T>(
        for factoryClosure: @escaping (AssetsExchangeOperationFactoryProtocol) -> CompoundOperationWrapper<T>
    ) -> CompoundOperationWrapper<T> {
        let graphWrapper = graphProvider.asyncWaitGraphWrapper()

        let targetWrapper = OperationCombiningService<T>.compoundNonOptionalWrapper(
            operationQueue: operationQueue
        ) {
            let graph = try graphWrapper.targetOperation.extractNoCancellableResultData()

            let operationFactory = AssetsExchangeOperationFactory(
                graph: graph,
                pathCostEstimator: self.pathCostEstimator,
                operationQueue: self.operationQueue,
                logger: self.logger
            )

            return factoryClosure(operationFactory)
        }

        targetWrapper.addDependency(wrapper: graphWrapper)

        return targetWrapper.insertingHead(operations: graphWrapper.allOperations)
    }
}

extension AssetsExchangeService: AssetsExchangeServiceProtocol {
    public func setup() {
        graphProvider.setup()
        feeSupportProvider.setup()
    }

    public func throttle() {
        graphProvider.throttle()
        feeSupportProvider.throttle()
    }

    public func subscribeUpdates(
        for target: AnyObject,
        notifyingIn queue: DispatchQueue,
        closure: @escaping (AssetsExchangeGraphProviderStats) -> Void
    ) {
        graphProvider.subscribeGraph(
            target,
            notifyingIn: queue
        ) { _, stats in
            closure(stats)
        }
    }

    public func unsubscribeUpdates(for target: AnyObject) {
        graphProvider.unsubscribeGraph(target)
    }

    public func fetchReachibilityWrapper() -> CompoundOperationWrapper<AssetsExchageGraphReachabilityProtocol> {
        let graphWrapper = graphProvider.asyncWaitGraphWrapper()

        let directionsOperation = ClosureOperation<AssetsExchageGraphReachabilityProtocol> {
            let graph = try graphWrapper.targetOperation.extractNoCancellableResultData()
            return graph.fetchReachability()
        }

        directionsOperation.addDependency(graphWrapper.targetOperation)

        return graphWrapper.insertingTail(operation: directionsOperation)
    }

    public func fetchAssetsInWrapper(
        given assetOutId: ChainAssetId?
    ) -> CompoundOperationWrapper<Set<ChainAssetId>> {
        let graphWrapper = graphProvider.asyncWaitGraphWrapper()

        let directionsOperation = ClosureOperation<Set<ChainAssetId>> {
            let graph = try graphWrapper.targetOperation.extractNoCancellableResultData()
            return graph.fetchAssetsIn(given: assetOutId)
        }

        directionsOperation.addDependency(graphWrapper.targetOperation)

        return graphWrapper.insertingTail(operation: directionsOperation)
    }

    public func fetchAssetsOutWrapper(
        given assetInId: ChainAssetId?
    ) -> CompoundOperationWrapper<Set<ChainAssetId>> {
        let graphWrapper = graphProvider.asyncWaitGraphWrapper()

        let directionsOperation = ClosureOperation<Set<ChainAssetId>> {
            let graph = try graphWrapper.targetOperation.extractNoCancellableResultData()
            return graph.fetchAssetsOut(given: assetInId)
        }

        directionsOperation.addDependency(graphWrapper.targetOperation)

        return graphWrapper.insertingTail(operation: directionsOperation)
    }

    public func fetchQuoteWrapper(
        for args: AssetConversion.QuoteArgs
    ) -> CompoundOperationWrapper<AssetExchangeQuote> {
        prepareWrapper { operationFactory in
            operationFactory.createQuoteWrapper(args: args)
        }
    }

    public func estimateFee(
        for args: AssetExchangeFeeArgs
    ) -> CompoundOperationWrapper<AssetExchangeFee> {
        prepareWrapper { $0.createFeeWrapper(for: args) }
    }

    public func submit(
        using estimation: AssetExchangeFee,
        creditingTo accountId: AccountId?,
        notifyingIn queue: DispatchQueue,
        operationStartClosure: @escaping (Int) -> Void
    ) -> CompoundOperationWrapper<Balance> {
        prepareWrapper {
            $0.createExecutionWrapper(
                for: estimation,
                creditingTo: accountId,
                notifyingIn: queue,
                operationStartClosure: operationStartClosure
            )
        }
    }

    public func submitSingleOperationWrapper(
        using estimation: AssetExchangeFee,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel> {
        prepareWrapper {
            $0.createSingleOperationSubmitWrapper(
                for: estimation,
                creditingTo: accountId
            )
        }
    }

    public func subscribeRequoteService(
        for target: AnyObject,
        ignoreIfAlreadyAdded: Bool,
        notifyingIn queue: DispatchQueue,
        closure: @escaping () -> Void
    ) {
        exchangesStateMediator.subscribeStateChanges(
            target,
            ignoreIfAlreadyAdded: ignoreIfAlreadyAdded,
            notifyingIn: queue,
            closure: closure
        )
    }

    public func throttleRequoteService() {
        exchangesStateMediator.throttleStateServicesSynchroniously()
    }

    public func canPayFee(in asset: ChainAssetProtocol) -> CompoundOperationWrapper<Bool> {
        guard !asset.assetInterface.isUtility else {
            return CompoundOperationWrapper.createWithResult(true)
        }

        let operation = AsyncClosureOperation<Bool>(operationClosure: { completionClosure in
            self.feeSupportProvider.fetchCurrentState(in: .global()) { state in
                let isFeeSupported = state?.canPayFee(inNonNative: asset.chainAssetId) ?? false
                completionClosure(.success(isFeeSupported))
            }
        })

        return CompoundOperationWrapper(targetOperation: operation)
    }
}
