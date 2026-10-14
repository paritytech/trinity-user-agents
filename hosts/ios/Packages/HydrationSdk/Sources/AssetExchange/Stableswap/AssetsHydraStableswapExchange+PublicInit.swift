import Foundation
import AssetExchange

public extension AssetsHydraStableswapExchange {
    convenience init(
        host: HydraExchangeHostProtocol,
        exchangeStateRegistrar: AssetsExchangeStateRegistring
    ) {
        let flowState = HydraStableswapFlowState(
            chain: host.chain,
            connection: host.connection,
            runtimeProvider: host.runtimeService,
            notificationsRegistrar: exchangeStateRegistrar,
            operationQueue: host.operationQueue,
            logger: host.logger
        )

        exchangeStateRegistrar.addStateProvider(flowState)

        self.init(
            host: host,
            swapFactory: .init(
                chain: host.chain,
                runtimeService: host.runtimeService,
                connection: host.connection,
                tokenConverter: host.tokenConverting,
                operationQueue: host.operationQueue
            ),
            quoteFactory: HydraStableswapQuoteFactory(flowState: flowState),
            logger: host.logger
        )
    }
}
