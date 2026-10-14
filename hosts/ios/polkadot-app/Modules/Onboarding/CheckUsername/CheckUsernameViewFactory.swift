import Foundation
import Keystore_iOS
import NovaCrypto
import ChainRegistry

@MainActor
enum CheckUsernameViewFactory {
    static func createView(with observer: RootStateObserving) -> CheckUsernameViewProtocol? {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let operationQueue = OperationManagerFacade.sharedDefaultQueue
        let chainId = AppConfig.Chains.usernameChain

        let identityService = IdentityService(
            chainRegistry: chainRegistry,
            chain: chainId,
            operationQueue: operationQueue,
            logger: Logger.shared
        )

        let walletRepo: WalletManagerRepositoryProtocol = .shared
        guard let selectedWallet = try? walletRepo.main() else {
            return nil
        }

        let interactor = CheckUsernameInteractor(
            selectedWallet: selectedWallet,
            identityService: identityService
        )

        let wireframe = CheckUsernameWireframe(observer: observer)
        let presenter = CheckUsernamePresenter(interactor: interactor, wireframe: wireframe)
        let view = CheckUsernameViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
