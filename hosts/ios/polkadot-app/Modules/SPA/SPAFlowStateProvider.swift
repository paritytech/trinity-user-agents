import Foundation
import os
import Products
import ChainRegistry
import Operation_iOS
import Revive
import BackgroundExecution

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
            metadataRepository: ProductMetadataRepository(contractApi: contractApi),
            hostProvider: hostProvider,
            logger: Logger.shared,
            screening: .core
        )

        let contentLoader = makeContentLoader(
            contractApi: contractApi,
            carFetcher: carFetcher,
            storage: DotNsLocalStorage(content: contentStorage, contentHashes: ContentHashCache.shared),
            hostProvider: hostProvider
        )

        let created = SPAFlowState(
            dotNsResolver: resolver,
            productImages: ProductImageSources(
                dotNsResolver: resolver,
                ipfsGatewayBaseUrl: config.ipfsGatewayBaseUrl
            ),
            hostProvider: hostProvider,
            productResolver: productResolver,
            contentLoader: contentLoader,
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

private extension SPAFlowStateProvider {
    func makeContentLoader(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        storage: DotNsLocalStorage,
        hostProvider: ProductHostProviding
    ) -> ProductContentLoading {
        guard let snapshotDefaults = UserDefaults(suiteName: DotNsRecordSnapshot.suiteName) else {
            fatalError("Can't create user defaults for the dotNS record snapshot")
        }

        let backgroundExecutor = ConnectionRetainingExecutor(provider: chainRegistry)

        return ProductContentLoader(
            fetcher: ProductUpdateFetcher(
                contractApi: contractApi,
                carFetcher: carFetcher,
                storage: storage,
                hostProvider: hostProvider,
                logger: Logger.shared
            ),
            cache: ProductVersionCache(
                snapshot: DotNsRecordSnapshot(defaults: snapshotDefaults),
                storage: storage,
                tldProvider: DotNsTldProviderFacade.shared,
                logger: Logger.shared
            ),
            environment: ProductContentLoader.Environment(
                pathProvider: ProductNetworkPath(),
                backgroundExecutor: backgroundExecutor,
                diagnostics: ProductLoadDiagnostics(reporter: IssueReportingFacade.shared)
            ),
            logger: Logger.shared
        )
    }
}
