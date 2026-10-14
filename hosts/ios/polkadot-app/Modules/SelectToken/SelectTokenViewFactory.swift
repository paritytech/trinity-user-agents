import Foundation
import SubstrateSdk
import ChainRegistry

@MainActor
enum SelectTokenViewFactory {
    static func createView(
        supportedTokens: [ChainAssetId],
        context: WalletFlowContextProtocol
    ) -> SelectTokenViewProtocol? {
        let wireframe = SelectTokenWireframe(context: context)

        let interactor = SelectTokenInteractor(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            supportedTokensService: SupportedTokensService(supportedTokens: supportedTokens)
        )

        let presenter = SelectTokenPresenter(
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: SelectTokenViewModelFactory(),
            logger: Logger.shared
        )

        let view = SelectTokenViewController(presenter: presenter)

        presenter.view = view
        interactor.tokensPresenter = presenter

        return view
    }
}
