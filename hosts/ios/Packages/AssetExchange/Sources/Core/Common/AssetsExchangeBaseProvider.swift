import Foundation
import SubstrateSdk
import Foundation_iOS
import ChainStore
import CommonService
import SDKLogger

open class AssetsExchangeBaseProvider {
    public let chainRegistry: ChainResourceProtocol
    public let graphProxy: AssetExchangeGraphProxy

    private var observableState: Observable<NotEqualWrapper<[AssetsExchangeProtocol]>> = .init(
        state: .init(value: [])
    )

    public let operationQueue: OperationQueue
    public let syncQueue: DispatchQueue
    public let logger: SDKLoggerProtocol

    public init(
        chainRegistry: ChainResourceProtocol,
        graphProxy: AssetExchangeGraphProxy,
        operationQueue: OperationQueue,
        syncQueue: DispatchQueue,
        logger: SDKLoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
        self.graphProxy = graphProxy
        self.syncQueue = syncQueue
        self.logger = logger
    }

    public convenience init(
        chainRegistry: ChainResourceProtocol,
        pathCostEstimator: AssetsExchangePathCostEstimating,
        operationQueue: OperationQueue,
        syncQueue: DispatchQueue,
        logger: SDKLoggerProtocol
    ) {
        self.init(
            chainRegistry: chainRegistry,
            graphProxy: AssetExchangeGraphProxy(
                pathCostEstimator: pathCostEstimator,
                operationQueue: operationQueue,
                logger: logger
            ),
            operationQueue: operationQueue,
            syncQueue: syncQueue,
            logger: logger
        )
    }

    public func updateState(with newExchanges: [AssetsExchangeProtocol]) {
        observableState.state = .init(value: newExchanges)
    }

    open func performSetup() {
        fatalError("Must be overriden by subsclass")
    }

    open func performThrottle() {
        fatalError("Must be overriden by subsclass")
    }
}

extension AssetsExchangeBaseProvider: AssetsExchangeProviding {
    public func setup() {
        performSetup()
    }

    public func throttle() {
        performThrottle()
    }

    public func subscribeExchanges(
        _ target: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping ([AssetsExchangeProtocol]) -> Void
    ) {
        syncQueue.async { [weak self] in
            self?.observableState.addObserver(
                with: target,
                sendStateOnSubscription: true,
                queue: queue
            ) { _, newState in
                onChange(newState.value)
            }
        }
    }

    public func unsubscribeExchanges(_ target: AnyObject) {
        syncQueue.async { [weak self] in
            self?.observableState.removeObserver(by: target)
        }
    }

    public func inject(graph: AssetsExchangeGraphProtocol) {
        graphProxy.install(graph: graph)
    }
}
