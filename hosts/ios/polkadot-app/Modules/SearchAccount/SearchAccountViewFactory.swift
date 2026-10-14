import Foundation
import Coinage
import ChainRegistry
import MessageExchangeKit
import Products

@MainActor
enum SearchAccountViewFactory {
    static func createView(
        for chainAsset: ChainAsset,
        coinageServicing: CoinageServicing,
        fundingDomainProvider: FundingDomainProviding
    ) -> SearchAccountViewProtocol? {
        let walletRepo: WalletManagerRepositoryProtocol = .shared

        guard let ownAccountId = try? walletRepo.main().getRawPublicKey() else {
            assertionFailure()
            return nil
        }

        let logger = Logger.shared
        let operationQueue = OperationManagerFacade.sharedDefaultQueue
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let recentContactsService = RecentContactsService(
            recentContactsSubscriptionFactory: RecentContactsSubscriptionFactory.shared,
            identityQueryFactory: IdentityPalletQueryFactory(
                operationQueue: operationQueue
            ),
            chainRegistry: chainRegistry,
            usernameChainId: AppConfig.Chains.usernameChain,
            operationQueue: operationQueue,
            logger: logger
        )
        let localContactSearch = LocalContactSearchService(
            repositoryFactory: ChatContactRepositoryFactory()
        )
        let recentRecipientsProvider = RecentRecipientsProvider(
            service: recentContactsService,
            chainFormat: chainAsset.chain.chainFormat,
            chainAssetId: chainAsset.chainAssetId,
            logger: logger
        )

        let accountSearching: any AccountSearching<
            RecentContactModelWithUsername,
            ContactSearchPayload
        > = AccountSearchProvider(
            recentRowsStream: { recentRecipientsProvider.subscribe() },
            localContactSearch: localContactSearch,
            remoteContactSearch: RemoteContactOperationFactory(),
            ownAccountId: ownAccountId,
            logger: logger
        )

        let recipientViewModelFactory = RecipientViewModelFactory()
        let interactor = SearchAccountInteractor(
            accountSearching: accountSearching,
            chatOpenResolver: ChatOpenModelResolver(),
            chainAsset: chainAsset,
            logger: logger,
            fundingDomainProvider: fundingDomainProvider
        )
        let wireframe = SearchAccountWireframe(coinageServicing: coinageServicing)
        let presenter = SearchAccountPresenter(
            interactor: interactor,
            wireframe: wireframe,
            recipientViewModelFactory: recipientViewModelFactory,
            chainAsset: chainAsset
        )

        let view = SearchAccountViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
