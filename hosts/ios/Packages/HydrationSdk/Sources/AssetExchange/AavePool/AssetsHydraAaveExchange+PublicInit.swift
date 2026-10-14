import Foundation
import AssetExchange

public extension AssetsHydraAaveExchange {
    convenience init(
        host: HydraExchangeHostProtocol,
        exchangeStateRegistrar: AssetsExchangeStateRegistring
    ) {
        let flowState = HydraAaveFlowState(
            connection: host.connection,
            runtimeProvider: host.runtimeService,
            notificationsRegistrar: exchangeStateRegistrar,
            operationQueue: host.operationQueue,
            logger: host.logger
        )

        exchangeStateRegistrar.addStateProvider(flowState)

        self.init(
            host: host,
            apiOperationFactory: HydraAaveTradeExecutorFactory(
                connection: host.connection,
                runtimeProvider: host.runtimeService,
                operationQueue: host.operationQueue
            ),
            quoteFactory: HydraAaveSwapQuoteFactory(flowState: flowState)
        )
    }
}
