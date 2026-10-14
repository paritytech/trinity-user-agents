import Foundation
import Keystore_iOS
import SubstrateSdk
import KeyDerivation
import PolkadotUI
import ChainRegistry

@MainActor
enum IdentityDetailsViewFactory {
    static func createEmbeddedScene(
        chainModel: ChainModel,
        wallet: WalletManaging
    ) -> IdentityDetailsScene {
        let scene = createScene(
            chainModel: chainModel,
            wallet: wallet
        )
        scene.setup()

        return scene
    }

    private static func createScene(
        chainModel: ChainModel,
        wallet: WalletManaging
    ) -> IdentityDetailsScene {
        let encoder = AddressQREncoder(addressFormat: .substrate(type: chainModel.addressPrefix))
        let operationFactory = QRCreationOperationFactory(chainStyle: nil)
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        let logger = Logger.shared
        let chainRegistry = ChainRegistryFacade.sharedRegistry

        let identityService = IdentityService(
            chainRegistry: chainRegistry,
            chain: AppConfig.Chains.usernameChain,
            operationQueue: operationQueue,
            logger: logger
        )

        let usernameStorage = UsernameStorage()
        let profileService = IdentityProfileService(
            usernameStorage: usernameStorage,
            identityService: identityService,
            wallet: wallet,
            logger: logger
        )

        let interactor = IdentityDetailsInteractor(
            shareFactory: AccountShareFactory(),
            profileService: profileService,
            chain: chainModel,
            wallet: wallet,
            qrEncoder: encoder,
            qrCodeCreationOperationFactory: operationFactory,
            logger: logger
        )

        let wireframe = IdentityDetailsWireframe()
        let presenter = IdentityDetailsPresenter(
            interactor: interactor,
            wireframe: wireframe,
            logger: logger
        )

        // Start from persisted data
        let viewModel = IdentityDetailsViewModel()
        viewModel.username = usernameStorage.username.map {
            .init(value: $0.value, isClaimed: usernameStorage.usernameClaimed)
        }

        viewModel.isRankVisible = false
        let binding = IdentityDetailsViewBinding(viewModel: viewModel)

        binding.bind(to: presenter)
        presenter.view = binding
        interactor.presenter = presenter

        return IdentityDetailsScene(
            viewModel: viewModel,
            binding: binding,
            presenter: presenter,
            interactor: interactor
        )
    }
}
