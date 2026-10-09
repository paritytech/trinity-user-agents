import Foundation
import os
import Products
import ChainRegistry
import Operation_iOS
import Revive

protocol SPAFlowStateProviding: AnyObject {
    func flowState() -> SPAFlowState
}

final class SPAFlowStateProvider: SPAFlowStateProviding {
    private struct State {
        var flowState: SPAFlowState?
    }

    private let state = OSAllocatedUnfairLock<State>(initialState: State())

    private let chainRegistry: ChainRegistryProtocol

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry
    ) {
        self.chainRegistry = chainRegistry
    }

    func flowState() -> SPAFlowState {
        if let cached = state.withLock({ $0.flowState }) {
            return cached
        }

        guard let config = try? AppConfig.DotNs.config() else {
            fatalError("DotNs config unavailable - remote config not loaded")
        }

        let contractApi = ReviveDotNsContractApi(
            contractApi: ReviveContractApi(
                chainId: config.contractsChainId,
                chainResource: chainRegistry,
                operationQueue: OperationManagerFacade.sharedDefaultQueue
            ),
            configProvider: { config }
        )

        let hostProvider = ProductHostFactory(tldProvider: DotNsTldProviderFacade.shared)

        let carFetcher = CarFetcher(gatewayBaseUrl: config.ipfsGatewayBaseUrl)
        let contentStorage = DotNsContentStorage()

        let resolver = DotNsResolver(
            contractApi: contractApi,
            carFetcher: carFetcher,
            contentStorage: contentStorage,
            contentHashCache: ContentHashCache.shared
        )

        let productResolver = ProductResolver(
            dotNsResolver: resolver,
            hostProvider: hostProvider,
            logger: Logger.shared,
            screening: .core
        )

        let created = SPAFlowState(
            dotNsResolver: resolver,
            hostProvider: hostProvider,
            productResolver: productResolver,
            iconViewModelFactory: ProductIconViewModelFactory(
                dotNsResolver: resolver,
                iconLoader: ProductIconLoader(productResolver: productResolver)
            )
        )

        return state.withLock { state in
            if let cached = state.flowState {
                return cached
            }
            state.flowState = created
            return created
        }
    }
}
