import Foundation

/// Assembles the SearchContact VIPER core. Both hosts — the scan panel and the full-screen
/// screen — build the module here, so the dependency graph exists once.
@MainActor
enum SearchContactModuleFactory {
    static func makeModule() -> (presenter: SearchContactPresenter, wireframe: SearchContactWireframe)? {
        let walletRepo: WalletManagerRepositoryProtocol = .shared
        guard let ownAccountId = try? walletRepo.main().getRawPublicKey() else {
            assertionFailure()
            return nil
        }

        let localContactSearch = LocalContactSearchService(
            repositoryFactory: ChatContactRepositoryFactory()
        )
        let recentChatsProvider = RecentChatsProvider(
            chatProvider: ChatContactDataProviderFactory()
        )

        let accountSearching: any AccountSearching<ContactSearchPayload, ContactSearchPayload> =
            AccountSearchProvider(
                recentRowsStream: { recentChatsProvider.subscribe() },
                localContactSearch: localContactSearch,
                remoteContactSearch: RemoteContactOperationFactory(),
                ownAccountId: ownAccountId,
                logger: Logger.shared
            )

        let interactor = SearchContactInteractor(accountSearching: accountSearching)
        let wireframe = SearchContactWireframe()
        let presenter = SearchContactPresenter(interactor: interactor, wireframe: wireframe)

        interactor.presenter = presenter

        return (presenter, wireframe)
    }
}
