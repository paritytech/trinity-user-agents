import Foundation
import AssetExchange

public extension AssetsHydraOmnipoolExchange {
    convenience init(
        host: HydraExchangeHostProtocol,
        exchangeStateRegistrar: AssetsExchangeStateRegistring
    ) {
        let flowState = HydraOmnipoolFlowState(
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
            tokensFactory: HydraOmnipoolTokensFactory(
                chain: host.chain,
                runtimeService: host.runtimeService,
                connection: host.connection,
                tokenConverter: host.tokenConverting,
                operationQueue: host.operationQueue
            ),
            quoteFactory: HydraOmnipoolQuoteFactory(flowState: flowState),
            logger: host.logger
        )
    }
}
