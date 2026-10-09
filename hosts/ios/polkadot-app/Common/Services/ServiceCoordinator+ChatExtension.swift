import Foundation
import Products
import KeyDerivation
import SubstrateSdk
import Individuality
import SubstrateStorageQuery
import Operation_iOS
import ChainRegistry

extension ServiceCoordinator {
    @MainActor
    static func createChatExtensionsRegistry(
        accountManager: ProductsAccountManaging,
        truapiRuntimeProvider: TrUAPIHostRuntimeProviding,
        spaFlowState: SPAFlowState,
        productFileProvider: any ChatProductFileProviding,
        pocket: ProductPocketService?
    ) -> (registry: ChatExtensionsRegistering, workerFacade: ProductWorkerFacade) {
        let productRepositoryFactory = ProductRepositoryFactory()

        // The builder gets the operations service (the worker's own JS uses it),
        // which lets the facade wire the factory into the manager in `init`.
        let workerFacade = ProductWorkerFacade { workerOperations in
            DefaultProductWorkerFactory(
                productResolver: spaFlowState.productResolver,
                dotNsResolver: spaFlowState.dotNsResolver,
                productFileProvider: productFileProvider,
                chainRegistry: ChainRegistryFacade.sharedRegistry,
                usernameStorage: UsernameStorage(),
                hostProvider: spaFlowState.hostProvider,
                accountManager: accountManager,
                workerOperations: workerOperations
            )
        }

        let botFactory = ProductBotFactory(
            productFileProvider: productFileProvider,
            runtimeProvider: truapiRuntimeProvider,
            workers: { pocket?.workers },
            workerManager: workerFacade.manager,
            productImages: spaFlowState.productImages
        )

        let productBotProvider = ProductBotProvider(
            productProvider: productRepositoryFactory.createProvider(),
            botFactory: botFactory,
            dotNsResolver: spaFlowState.dotNsResolver,
            productResolver: spaFlowState.productResolver
        )

        let registry = MainActor.assumeIsolated {
            ChatExtensionsRegistry.createDefault(
                productBotProvider: productBotProvider
            )
        }

        // Detached from the registry: the row is the host's own, drawn before the product runs.
        // It gives up rather than waiting forever, so a second assembly leaves no task behind.
        Task { await HostPlacedRoomPlacer().placeRooms() }

        return (registry, workerFacade)
    }
}
