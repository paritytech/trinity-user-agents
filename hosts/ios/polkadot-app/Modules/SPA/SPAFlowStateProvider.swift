import Foundation
import os
import IssueMonitoring
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
        let telemetry = HostLoadTelemetry(recorder: IssueMonitoringFactory.createHostEventRecorder())

        let resolver = DotNsResolver(
            contractApi: contractApi,
            carFetcher: carFetcher,
            contentStorage: contentStorage,
            contentHashCache: ContentHashCache.shared,
            telemetry: telemetry
        )

        let productResolver = ProductResolver(
            metadataRepository: ProductMetadataRepository(contractApi: contractApi),
            hostProvider: hostProvider,
            logger: Logger.shared,
            screening: .core,
            telemetry: telemetry
        )

        let contentLoader = makeContentLoader(
            contractApi: contractApi,
            carFetcher: carFetcher,
            storage: DotNsLocalStorage(content: contentStorage, contentHashes: ContentHashCache.shared),
            hostProvider: hostProvider,
            telemetry: telemetry
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
            ),
            telemetry: telemetry
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
        hostProvider: ProductHostProviding,
        telemetry: HostLoadTelemetry
    ) -> ProductContentLoading {
        let backgroundExecutor = ConnectionRetainingExecutor(provider: chainRegistry)
        let resolverFactory = ProductResolverFactory(screening: .core, logger: Logger.shared)

        return ProductContentLoader(
            fetcher: ProductUpdateFetcher(
                contractApi: contractApi,
                carFetcher: carFetcher,
                storage: storage,
                hostProvider: hostProvider,
                resolverFactory: resolverFactory,
                telemetry: telemetry
            ),
            cache: ProductContentCache(
                store: ProductManifestFileStore(),
                storage: storage,
                tldProvider: DotNsTldProviderFacade.shared,
                resolverFactory: resolverFactory,
                logger: Logger.shared
            ),
            environment: ProductContentLoader.Environment(
                pathProvider: ProductNetworkPath(),
                backgroundExecutor: backgroundExecutor,
                diagnostics: ProductLoadDiagnostics(reporter: IssueReportingFacade.shared)
            ),
            telemetry: telemetry,
            logger: Logger.shared
        )
    }
}
