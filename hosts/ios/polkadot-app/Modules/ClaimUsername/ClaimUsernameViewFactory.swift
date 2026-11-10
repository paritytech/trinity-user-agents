import Foundation
import Common
import Keystore_iOS
import ExtrinsicService
import KeyDerivation
import ChainRegistry
import Individuality
import SubstrateSdk
import SubstrateStorageQuery
import Operation_iOS

@MainActor
enum ClaimUsernameViewFactory {
    static func createLiteClaimView(
        observer: RootStateObserving
    ) -> ClaimUsernameViewProtocol? {
        guard let hasWallets = try? RootEntropyManager.shared.hasRootEntropy() else {
            return nil
        }

        guard let interactor = createLiteInteractor(hasWallets: hasWallets) else {
            return nil
        }

        let wireframe = ClaimLiteUsernameWireframe(observer: observer)

        let validationFactory = UsernameValidationFactory(presentable: wireframe)
        let presenter = ClaimUsernamePresenter(
            interactor: interactor,
            wireframe: wireframe,
            validationFactory: validationFactory,
            viewModelProvider: ClaimUsernameViewModelFactory(
                recoverable: !hasWallets,
                full: false
            ),
            prefilledUsername: nil,
            logger: Logger.shared
        )

        let view = ClaimLiteUsernameViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter
        validationFactory.view = view

        return view
    }

    private static func createLiteInteractor(hasWallets: Bool) -> ClaimLiteUsernameInteractor? {
        let operationQueue = OperationManagerFacade.sharedDefaultQueue
        let timeProvider = ChainTimeProvider(
            chainId: AppConfig.Chains.chatChain,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            storageRequestFactory: StorageRequestFactory(
                remoteFactory: StorageKeyFactory(),
                operationManager: OperationManager(operationQueue: operationQueue)
            )
        )
        let dependencies = ClaimLiteUsernameDependency(
            walletSetupManagerFactory: { createWalletManager() },
            registrationParamsFactory: { mainWallet, liteVrfManager in
                try LitePersonParamsFactory(
                    mainWallet: mainWallet,
                    liteVrfManager: liteVrfManager,
                    chatEncryptorManager: ChatEncryptionManager()
                )
            },
            chainTimeProvider: { timeProvider },
            usernameOperationFactory: { UsernameOperationFactory(tokenProvider: JWTTokenManager.shared) },
            usernameStorage: { UsernameStorage() },
            walletRepo: .shared,
            vrfRepo: .shared
        )

        return ClaimLiteUsernameInteractor(
            walletCreated: hasWallets,
            dependencies: dependencies,
            logger: Logger.shared
        )
    }

    private static func createWalletManager() -> WalletSetupManaging {
        WalletSetupManager(
            mnemonicGenerator: IRMnemonicCreator(),
            mnemonicBackupHelper: MnemonicBackupHelper(),
            entropyManager: RootEntropyManager.shared,
            logger: Logger.shared
        )
    }
}
