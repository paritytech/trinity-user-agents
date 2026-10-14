import Foundation
import AssetExchange

public extension AssetsHydraXYKExchange {
    convenience init(
        host: HydraExchangeHostProtocol,
        exchangeStateRegistrar: AssetsExchangeStateRegistring
    ) {
        let flowState = HydraXYKFlowState(
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
            tokensFactory: .init(
                chain: host.chain,
                runtimeService: host.runtimeService,
                connection: host.connection,
                tokenConverter: host.tokenConverting,
                operationQueue: host.operationQueue
            ),
            quoteFactory: .init(flowState: flowState),
            logger: host.logger
        )
    }
}
