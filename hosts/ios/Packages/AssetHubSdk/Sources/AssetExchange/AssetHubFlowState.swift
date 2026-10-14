import Foundation
import SubstrateSdk
import Operation_iOS
import AssetExchange
import SDKLogger

public protocol AssetHubFlowStateProtocol {
    func setupReQuoteService() -> AssetHubReQuoteService
}

public final class AssetHubFlowState {
    private let connection: JSONRPCEngine
    private let runtimeProvider: RuntimeCodingServiceProtocol
    private let operationQueue: OperationQueue
    private let notificationsRegistrar: AssetsExchangeStateRegistring?
    private let logger: SDKLoggerProtocol

    private let mutex = NSLock()

    private var reQuoteService: AssetHubReQuoteService?

    public init(
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        notificationsRegistrar: AssetsExchangeStateRegistring?,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.connection = connection
        self.runtimeProvider = runtimeProvider
        self.notificationsRegistrar = notificationsRegistrar
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension AssetHubFlowState: AssetHubFlowStateProtocol {
    public func setupReQuoteService() -> AssetHubReQuoteService {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let reQuoteService {
            return reQuoteService
        }

        let service = AssetHubReQuoteService(
            connection: connection,
            runtimeProvider: runtimeProvider,
            operationQueue: operationQueue,
            logger: logger
        )

        reQuoteService = service
        service.setup()

        notificationsRegistrar?.registerStateService(service)

        return service
    }
}

extension AssetHubFlowState: AssetsExchangeStateProviding {
    public func throttleStateServices() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let reQuoteService {
            notificationsRegistrar?.deregisterStateService(reQuoteService)
            reQuoteService.throttle()
        }

        reQuoteService = nil
    }
}
