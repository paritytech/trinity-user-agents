import Foundation
import ChainRegistry
import Individuality
import KeyDerivation
import Products

/// Creates per-caller Accounts Protocol handlers (RFC-0004, RFC-0023) with prompt routing baked in.
protocol APPersonhoodHandlerMaking: Sendable {
    func makeCreateProofHandler(callingProductId: ProductId?) -> APCreateProofHandling
    func makeAliasHandler(callingProductId: ProductId?) -> APAliasHandling
    func makeSignVrfHandler(callingProductId: ProductId?) -> APSignVrfHandling
}

/// Single wiring point for Accounts Protocol handlers, shared between
/// the in-WebView host API and the SSO request handlers. Prompt routing
/// comes from the caller's router facade so confirmations anchor to that
/// context's presentation view.
final class APPersonhoodHandlerFactory: @unchecked Sendable {
    private let chainRegistry: ChainRegistryProtocol
    private let routers: ProductRoutersFacadeProtocol
    private let entropyManager: RootEntropyManaging
    private let tldProvider: DotNsTldProviding
    private let permissionRepository: ProductPermissionRepositoryProtocol
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        routers: ProductRoutersFacadeProtocol,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        permissionRepository: ProductPermissionRepositoryProtocol = ProductPermissionRepository(),
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chainRegistry = chainRegistry
        self.routers = routers
        self.entropyManager = entropyManager
        self.tldProvider = tldProvider
        self.permissionRepository = permissionRepository
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension APPersonhoodHandlerFactory: APPersonhoodHandlerMaking {
    func makeCreateProofHandler(callingProductId: ProductId?) -> APCreateProofHandling {
        APCreateProofHandler(
            callingProductId: callingProductId,
            options: makeOptions(),
            depsResolver: makeDepsResolver(),
            confirmationRequester: CreateProofConfirmationRequester(
                router: routers.productsRouter
            ),
            logger: logger
        )
    }

    func makeSignVrfHandler(callingProductId: ProductId?) -> APSignVrfHandling {
        APSignVrfHandler(
            callingProductId: callingProductId,
            confirmationRequester: SignVrfConfirmationRequester(router: routers.productsRouter),
            walletFactory: { try DynamicDerivedWallet(derivationPath: $0.derivationPath()) },
            logger: logger
        )
    }

    func makeAliasHandler(callingProductId: ProductId?) -> APAliasHandling {
        APAliasHandler(
            callingProductId: callingProductId,
            options: makeOptions(),
            depsResolver: makeDepsResolver(),
            accountAccessHandler: AccountAccessPermissionHandler(
                repository: permissionRepository,
                requester: ProductPermissionRequesterFactory.create(router: routers.productsRouter)
            ),
            logger: logger
        )
    }
}

private extension APPersonhoodHandlerFactory {
    func makeDepsResolver() -> MembershipDepsResolving {
        MembershipDepsResolver(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue
        )
    }

    // Priority order: the full-person ("PoP") key first, then lite.
    func makeOptions() -> [CreateProofOrAliasOption] {
        guard let tld = try? tldProvider.currentTldOrError() else {
            logger.error("Personhood options unavailable: DotNs TLD not resolved")
            return []
        }

        return [
            CreateProofOrAliasOption(
                collectionId: PeoplePallet.membersIdentifier,
                keyManager: BandersnatchKeyManager.fullPerson(for: tld, entropyManager: entropyManager)
            ),
            CreateProofOrAliasOption(
                collectionId: PeopleLitePallet.membersIdentifier,
                keyManager: BandersnatchKeyManager.litePerson(for: tld, entropyManager: entropyManager)
            )
        ]
    }
}
