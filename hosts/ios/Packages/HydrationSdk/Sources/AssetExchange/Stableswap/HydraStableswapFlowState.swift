import Foundation
import SubstrateSdk
import Operation_iOS
import AssetExchange
import SDKLogger

final class HydraStableswapFlowState {
    let chain: ChainProtocol
    let connection: JSONRPCEngine
    let runtimeProvider: RuntimeCodingServiceProtocol
    let operationQueue: OperationQueue
    let notificationsRegistrar: AssetsExchangeStateRegistring?
    let logger: SDKLoggerProtocol

    let mutex = NSLock()

    private var quoteStateServices: [HydraStableswap.PoolPair: HydraStableswapQuoteParamsService] = [:]

    init(
        chain: ChainProtocol,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        notificationsRegistrar: AssetsExchangeStateRegistring?,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.chain = chain
        self.connection = connection
        self.runtimeProvider = runtimeProvider
        self.notificationsRegistrar = notificationsRegistrar
        self.operationQueue = operationQueue
        self.logger = logger
    }

    deinit {
        quoteStateServices.values.forEach {
            notificationsRegistrar?.deregisterStateService($0)
            $0.throttle()
        }
    }
}

extension HydraStableswapFlowState {
    func resetServices() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        quoteStateServices.values.forEach {
            notificationsRegistrar?.deregisterStateService($0)
            $0.throttle()
        }

        quoteStateServices = [:]
    }

    func setupQuoteService(
        for poolPair: HydraStableswap.PoolPair
    ) -> HydraStableswapQuoteParamsService {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let currentService = quoteStateServices[poolPair] {
            return currentService
        }

        let newService = HydraStableswapQuoteParamsService(
            poolAsset: poolPair.poolAsset,
            assetIn: poolPair.assetIn,
            assetOut: poolPair.assetOut,
            connection: connection,
            runtimeProvider: runtimeProvider,
            operationQueue: operationQueue,
            logger: logger
        )

        quoteStateServices[poolPair] = newService

        newService.setup()

        notificationsRegistrar?.registerStateService(newService)

        return newService
    }
}

extension HydraStableswapFlowState: AssetsExchangeStateProviding {
    func throttleStateServices() {
        resetServices()
    }
}
