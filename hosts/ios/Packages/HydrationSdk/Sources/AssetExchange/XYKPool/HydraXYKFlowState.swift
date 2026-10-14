import Foundation
import SubstrateSdk
import Operation_iOS
import AssetExchange
import SDKLogger

final class HydraXYKFlowState {
    let chain: ChainProtocol
    let connection: JSONRPCEngine
    let runtimeProvider: RuntimeCodingServiceProtocol
    let operationQueue: OperationQueue
    let notificationsRegistrar: AssetsExchangeStateRegistring?
    let workQueue: DispatchQueue
    let logger: SDKLoggerProtocol

    let mutex = NSLock()

    private var quoteStateServices: [HydraDx.RemoteSwapPair: HydraXYKQuoteParamsService] = [:]

    init(
        chain: ChainProtocol,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        notificationsRegistrar: AssetsExchangeStateRegistring?,
        operationQueue: OperationQueue,
        workQueue: DispatchQueue = .global(),
        logger: SDKLoggerProtocol
    ) {
        self.chain = chain
        self.connection = connection
        self.runtimeProvider = runtimeProvider
        self.notificationsRegistrar = notificationsRegistrar
        self.operationQueue = operationQueue
        self.workQueue = workQueue
        self.logger = logger
    }

    deinit {
        quoteStateServices.values.forEach {
            notificationsRegistrar?.deregisterStateService($0)
            $0.throttle()
        }
    }
}

extension HydraXYKFlowState {
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

    func setupQuoteService(for swapPair: HydraDx.RemoteSwapPair) -> HydraXYKQuoteParamsService {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let currentService = quoteStateServices[swapPair] {
            return currentService
        }

        let newService = HydraXYKQuoteParamsService(
            chain: chain,
            assetIn: swapPair.assetIn,
            assetOut: swapPair.assetOut,
            connection: connection,
            runtimeProvider: runtimeProvider,
            operationQueue: operationQueue,
            workQueue: workQueue,
            logger: logger
        )

        quoteStateServices[swapPair] = newService

        newService.setup()

        notificationsRegistrar?.registerStateService(newService)

        return newService
    }
}

extension HydraXYKFlowState: AssetsExchangeStateProviding {
    func throttleStateServices() {
        resetServices()
    }
}
