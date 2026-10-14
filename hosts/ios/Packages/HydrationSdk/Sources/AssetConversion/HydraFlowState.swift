import Foundation
import SubstrateSdk
import Operation_iOS
import SDKLogger

final class HydraFlowState {
    let chain: ChainProtocol
    let connection: JSONRPCEngine
    let runtimeProvider: RuntimeCodingServiceProtocol
    let tokenConverter: HydrationTokenConverting
    let operationQueue: OperationQueue
    let logger: SDKLoggerProtocol

    let mutex = NSLock()

    private var omnipoolFlowState: HydraOmnipoolFlowState?
    private var stableswapFlowState: HydraStableswapFlowState?
    private var xykswapFlowState: HydraXYKFlowState?
    private var aaveFlowState: HydraAaveFlowState?
    private var routesFactory: HydraRoutesOperationFactoryProtocol?

    private var currentSwapPair: HydraDx.LocalSwapPair?

    init(
        chain: ChainProtocol,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        tokenConverter: HydrationTokenConverting,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.chain = chain
        self.connection = connection
        self.runtimeProvider = runtimeProvider
        self.tokenConverter = tokenConverter
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension HydraFlowState {
    func resetServicesIfNotMatchingPair(_ swapPair: HydraDx.LocalSwapPair) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard swapPair != currentSwapPair else {
            return
        }

        omnipoolFlowState?.resetServices()
        stableswapFlowState?.resetServices()
        xykswapFlowState?.resetServices()

        routesFactory = nil

        currentSwapPair = swapPair
    }

    func getOmnipoolFlowState() -> HydraOmnipoolFlowState {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let state = omnipoolFlowState {
            return state
        }

        let newState = HydraOmnipoolFlowState(
            chain: chain,
            connection: connection,
            runtimeProvider: runtimeProvider,
            notificationsRegistrar: nil,
            operationQueue: operationQueue,
            logger: logger
        )

        omnipoolFlowState = newState

        return newState
    }

    func getStableswapFlowState() -> HydraStableswapFlowState {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let state = stableswapFlowState {
            return state
        }

        let newState = HydraStableswapFlowState(
            chain: chain,
            connection: connection,
            runtimeProvider: runtimeProvider,
            notificationsRegistrar: nil,
            operationQueue: operationQueue,
            logger: logger
        )

        stableswapFlowState = newState

        return newState
    }

    func getXYKSwapFlowState() -> HydraXYKFlowState {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let state = xykswapFlowState {
            return state
        }

        let newState = HydraXYKFlowState(
            chain: chain,
            connection: connection,
            runtimeProvider: runtimeProvider,
            notificationsRegistrar: nil,
            operationQueue: operationQueue,
            logger: logger
        )

        xykswapFlowState = newState

        return newState
    }

    func getAaveSwapFlowState() -> HydraAaveFlowState {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let state = aaveFlowState {
            return state
        }

        let newState = HydraAaveFlowState(
            connection: connection,
            runtimeProvider: runtimeProvider,
            notificationsRegistrar: nil,
            operationQueue: operationQueue,
            logger: logger
        )

        aaveFlowState = newState

        return newState
    }

    func getRoutesFactory() -> HydraRoutesOperationFactoryProtocol {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let factory = routesFactory {
            return factory
        }

        let factory = HydraRoutesOperationFactory(
            chain: chain,
            connection: connection,
            runtimeProvider: runtimeProvider,
            tokenConverter: tokenConverter,
            operationQueue: operationQueue
        )

        routesFactory = factory

        return factory
    }
}
