import Foundation

/// Assembles the chat contact search for a product's pick: the same screen,
/// with the stored contacts as its only source and the product's answer as its
/// ending.
@MainActor
enum ContactPickViewFactory {
    static func createView(context: ContactPickContext) -> SearchContactViewProtocol? {
        let walletRepo: WalletManagerRepositoryProtocol = .shared
        guard let ownAccountId = try? walletRepo.main().getRawPublicKey() else {
            return nil
        }

        let interactor = SearchContactInteractor(
            accountSearching: ContactPickSearchProvider(
                localContactSearch: LocalContactSearchService(
                    repositoryFactory: ChatContactRepositoryFactory()
                ),
                ownAccountId: ownAccountId
            )
        )
        let presenter = SearchContactPresenter(
            interactor: interactor,
            wireframe: ContactPickWireframe(context: context)
        )
        let view = SearchContactViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
