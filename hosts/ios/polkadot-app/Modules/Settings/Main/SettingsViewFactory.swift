import Foundation
import Keystore_iOS
import Foundation_iOS
import DesignSystem

@MainActor
enum SettingsViewFactory {
    static func createView(
        serviceCoordinator: ServiceCoordinatorProtocol,
        flowStateProvider: any SPAFlowStateProviding
    ) -> SettingsViewProtocol? {
        let interactor = SettingsInteractor(
            logger: Logger.shared,
            mnemonicBackupHelper: MnemonicBackupHelper()
        )

        let wireframe = SettingsWireframe(
            serviceCoordinator: serviceCoordinator,
            flowStateProvider: flowStateProvider
        )
        let presenter = SettingsPresenter(
            interactor: interactor,
            wireframe: wireframe,
            viewModelFactory: SettingsViewModelFactory(),
            themeManager: ThemeManager.shared
        )

        let view = SettingsViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
