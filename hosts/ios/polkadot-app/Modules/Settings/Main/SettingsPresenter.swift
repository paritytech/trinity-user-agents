import DesignSystem
import UIKit
import UIKitExt
import Coinage

@MainActor
final class SettingsPresenter {
    weak var view: SettingsViewProtocol?
    let wireframe: SettingsWireframeProtocol
    let interactor: SettingsInteractorInputProtocol
    let viewModelFactory: SettingsViewModelMaking
    let themeManager: ThemeManagerProtocol

    private var attentionItems: Set<SettingsViewModel.CellType> = []
    private var hasBlockedUsers = false
    private var appVersion: String?
    private var selectedCurrencyCode: String?
    private var selectedPrivacyStrategy: RecyclingStrategyType?
    private var isTabBarLabelsEnabled = false

    init(
        interactor: SettingsInteractorInputProtocol,
        wireframe: SettingsWireframeProtocol,
        viewModelFactory: SettingsViewModelMaking,
        themeManager: ThemeManagerProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.themeManager = themeManager
    }
}

private extension SettingsPresenter {
    func visibleCells() -> Set<SettingsViewModel.CellType> {
        let all = Set(SettingsViewModel.Section.allCases.flatMap(\.cells))
        return hasBlockedUsers ? all : all.subtracting([.blockedUsers])
    }

    var selectedThemeName: String {
        switch themeManager.mode {
        case let .app(selection): selection.displayName
        }
    }

    func refreshContent() {
        let input = SettingsContentInput(
            visibleCells: visibleCells(),
            attentionItems: attentionItems,
            selectedCurrencyCode: selectedCurrencyCode,
            selectedThemeName: selectedThemeName,
            selectedPrivacyStrategy: selectedPrivacyStrategy,
            appVersion: appVersion,
            isTabBarLabelsEnabled: isTabBarLabelsEnabled,
            onSelect: { [weak self] cellType in
                self?.didTapCell(cellType)
            },
            onSelectPrivacyStrategy: { [weak self] strategy in
                self?.didSelectPrivacyStrategy(strategy)
            },
            onToggleTabBarLabels: { [weak self] isEnabled in
                self?.didToggleTabBarLabels(isEnabled)
            }
        )
        view?.applyContent(viewModelFactory.makeContent(input))
    }

    func didToggleTabBarLabels(_ isEnabled: Bool) {
        guard isEnabled != isTabBarLabelsEnabled else { return }
        isTabBarLabelsEnabled = isEnabled
        refreshContent()
        interactor.saveTabBarLabelsEnabled(isEnabled)
    }
}

extension SettingsPresenter: SettingsPresenterProtocol {
    func setup() {
        interactor.setup()
    }

    func didSelectPrivacyStrategy(_ strategy: RecyclingStrategyType) {
        guard strategy != selectedPrivacyStrategy else { return }
        selectedPrivacyStrategy = strategy
        refreshContent()
        interactor.savePrivacyStrategy(strategy)
    }

    func didTapCell(_ cell: SettingsViewModel.CellType) {
        switch cell {
        case .legalSupport:
            wireframe.showLegalSupport(from: view)
        case .backup:
            wireframe.showBackupFlow(from: view)
        case .theme:
            wireframe.showThemeSelection(from: view) { [weak self] in
                self?.refreshContent()
            }
        case .tabBarLabels:
            break
        case .currency:
            wireframe.showCurrencyPicker(from: view)
        case .linkedDevices:
            wireframe.showLinkedDevices(from: view)
        case .apps:
            wireframe.showApps(from: view)
        case .blockedUsers:
            wireframe.showBlockedUsers(from: view)
        }
    }
}

extension SettingsPresenter: SettingsInteractorOutputProtocol {
    func didReceiveAppVersion(_ appInfo: (version: String, build: String)) {
        appVersion = "v\(appInfo.version) (\(appInfo.build))"
        refreshContent()
    }

    func didReceiveBackupAttention(isRequired: Bool) {
        if isRequired {
            attentionItems.insert(.backup)
        } else {
            attentionItems.remove(.backup)
        }
        refreshContent()
    }

    func didReceiveSelectedCurrency(_ code: String) {
        selectedCurrencyCode = code
        refreshContent()
    }

    func didReceiveHasBlockedUsers(_ hasBlockedUsers: Bool) {
        self.hasBlockedUsers = hasBlockedUsers
        refreshContent()
    }

    func didReceivePrivacyStrategy(_ strategy: RecyclingStrategyType) {
        guard strategy != selectedPrivacyStrategy else { return }
        selectedPrivacyStrategy = strategy
        refreshContent()
    }

    func didReceiveTabBarLabelsEnabled(_ isEnabled: Bool) {
        guard isEnabled != isTabBarLabelsEnabled else { return }
        isTabBarLabelsEnabled = isEnabled
        refreshContent()
    }
}
