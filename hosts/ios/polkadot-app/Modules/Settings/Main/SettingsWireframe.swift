import Foundation
import UIKit
import UIKitExt

@MainActor
final class SettingsWireframe: SettingsWireframeProtocol {
    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let flowStateProvider: any SPAFlowStateProviding

    init(
        serviceCoordinator: ServiceCoordinatorProtocol,
        flowStateProvider: any SPAFlowStateProviding
    ) {
        self.serviceCoordinator = serviceCoordinator
        self.flowStateProvider = flowStateProvider
    }

    func showBackupFlow(from view: (any SettingsViewProtocol)?) {
        guard let backup = BackupViewFactory.createView() else {
            return
        }

        view?.controller.navigationController?.pushViewController(backup.controller, animated: true)
    }

    func showLinkedDevices(from view: (any SettingsViewProtocol)?) {
        guard let linkedDevicesView = LinkedDevicesViewFactory.createView(
            serviceCoordinator: serviceCoordinator
        ) else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            linkedDevicesView.controller,
            animated: true
        )
    }

    func showCurrencyPicker(from view: (any SettingsViewProtocol)?) {
        let pickerController = CurrencyPickerViewFactory.createView()
        view?.controller.navigationController?.pushViewController(pickerController, animated: true)
    }

    func showLegalSupport(from view: (any SettingsViewProtocol)?) {
        let legalSupportView = LegalSupportViewFactory.createView()

        view?.controller.navigationController?.pushViewController(
            legalSupportView.controller,
            animated: true
        )
    }

    func showBlockedUsers(from view: (any SettingsViewProtocol)?) {
        guard let blockedUsersView = BlockedUsersViewFactory.createView() else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            blockedUsersView.controller,
            animated: true
        )
    }

    func showApps(from view: (any SettingsViewProtocol)?) {
        guard let appsView = AppsListViewFactory.createView(flowStateProvider: flowStateProvider) else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            appsView.controller,
            animated: true
        )
    }

    func showThemeSelection(
        from view: (any SettingsViewProtocol)?,
        onFinish: @escaping () -> Void
    ) {
        guard let view else { return }

        let controller = ThemeSelectionViewFactory.createView { [weak view] in
            view?.controller.navigationController?.popViewController(animated: true)
            onFinish()
        }

        controller.hidesBottomBarWhenPushed = true

        view.controller.navigationController?.pushViewController(controller, animated: true)
    }
}
