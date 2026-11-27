import Foundation
import Keystore_iOS
import KeyDerivation
import ChainRegistry

@MainActor
enum BackupViewFactory {
    static func createView(serviceCoordinator: ServiceCoordinatorProtocol) -> BackupViewProtocol? {
        let chainAssetId = AppConfig.Assets.mainAsset

        guard
            let chain = ChainRegistryFacade.sharedRegistry.getChain(for: chainAssetId.chainId),
            let chainAsset = chain.chainAsset(for: chainAssetId.assetId)
        else {
            return nil
        }

        let logger = Logger.shared
        let interactor = BackupInteractor(
            cloudKeychain: SynchronizableKeychain(),
            entropyManager: RootEntropyManager.shared,
            coinageService: serviceCoordinator.coinageService,
            coinageBackupSyncService: serviceCoordinator.coinageBackupSyncService,
            logger: logger
        )
        let wireframe = BackupWireframe()

        let presenter = BackupPresenter(
            interactor: interactor,
            wireframe: wireframe,
            chainAsset: chainAsset,
            logger: logger
        )

        let view = BackupViewController(presenter: presenter)
        view.controller.navigationItem.largeTitleDisplayMode = .never
        view.controller.hidesBottomBarWhenPushed = true

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
