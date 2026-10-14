import Foundation
import UIKitExt
import Coinage

protocol SettingsViewProtocol: ControllerBackedProtocol {
    func applyContent(_ content: SettingsViewModel.Content)
}

@MainActor
protocol SettingsPresenterProtocol: AnyObject {
    func setup()
    func didTapCell(_ cell: SettingsViewModel.CellType)
    func didSelectPrivacyStrategy(_ strategy: RecyclingStrategyType)
}

protocol SettingsInteractorInputProtocol: AnyObject {
    func setup()
    func savePrivacyStrategy(_ strategy: RecyclingStrategyType)
    func saveTabBarLabelsEnabled(_ isEnabled: Bool)
}

@MainActor
protocol SettingsInteractorOutputProtocol: AnyObject {
    func didReceiveAppVersion(_ appInfo: (version: String, build: String))
    func didReceiveBackupAttention(isRequired: Bool)
    func didReceiveSelectedCurrency(_ code: String)
    func didReceiveHasBlockedUsers(_ hasBlockedUsers: Bool)
    func didReceivePrivacyStrategy(_ strategy: RecyclingStrategyType)
    func didReceiveTabBarLabelsEnabled(_ isEnabled: Bool)
}

@MainActor
protocol SettingsWireframeProtocol: AnyObject, WebPresentable, AlertPresentable {
    func showBackupFlow(from view: SettingsViewProtocol?)
    func showLinkedDevices(from view: SettingsViewProtocol?)
    func showCurrencyPicker(from view: SettingsViewProtocol?)
    func showLegalSupport(from view: SettingsViewProtocol?)
    func showBlockedUsers(from view: SettingsViewProtocol?)
    func showApps(from view: SettingsViewProtocol?)
    func showThemeSelection(from view: SettingsViewProtocol?, onFinish: @escaping () -> Void)
}
