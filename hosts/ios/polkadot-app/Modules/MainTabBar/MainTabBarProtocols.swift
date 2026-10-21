import UIKit
import PolkadotUI
import UIKitExt
import ExternalAccessibility
import Products

protocol MainTabBarViewProtocol: ControllerBackedProtocol, AppWidgetManaging {
    func show(slots: [TabBarSlot], selecting tab: TabBarItem)
    func select(tab: TabBarItem)
    #if !FEATURE_PRODUCTS
        func setAttention(_ isVisible: Bool, for tab: TabBarItem)
    #endif
    func setLabels(visible: Bool)
    func showSPATabs(_ viewModels: [SPATabChipViewModel])
    func showChainStatus(_ rows: [ChainConnectionStatusViewModel])
    func showProductGamePill(_ viewModel: ProductGamePillViewModel?)
}

@MainActor
protocol MainTabBarPresenterProtocol: AnyObject {
    func setup()
    func configureViews()
    func didRequestConnectionStatus()
    func didRequestSettings()
    #if FEATURE_INPUT
        func didFindChat(_ model: ChatOpenModel)
    #else
        func didRequestContactSearch()
    #endif
}

protocol MainTabBarInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol MainTabBarInteractorOutputProtocol: AnyObject {
    #if !FEATURE_PRODUCTS
        func didUpdateSettingsAttention(isVisible: Bool)
    #endif
    func didReceiveWidget(
        configuration: any HashableContentConfiguration,
        for extensionId: ChatExtension.Id
    )
    func didRemoveWidget(for extensionId: ChatExtension.Id)
    func didReceivePolkadotSignInRequest(with url: URL)
    func didReceiveSPATabs(_ tabs: [SPATab])
    func didReceiveChainStatus(_ rows: [ChainConnectionStatusViewModel])
    func didReceiveTabBarLabelsEnabled(_ isEnabled: Bool)
    func didReceiveProductGamePill(_ pill: ProductGamePill?)
}

@MainActor
protocol MainTabBarWireframeProtocol: AnyObject {
    func showPolkadotSignIn(with url: URL, view: MainTabBarViewProtocol?)
    func openProduct(productId: ProductId)
    #if FEATURE_INPUT
        func openChat(_ model: ChatOpenModel)
    #else
        func showSearchContact(from view: MainTabBarViewProtocol?)
    #endif
    func showConnectionStatus(_ viewModel: ConnectionStatusSheetViewModel, from view: MainTabBarViewProtocol?)
    func showSettings(from view: MainTabBarViewProtocol?)
}

enum TabBarItem: String, CaseIterable {
    case chat
    case wallet
    case browse
    case settings

    var image: UIImage {
        let asset: UIImage =
            switch self {
            case .chat: .tabChat
            case .wallet: .tabWallet
            case .browse: .tabBrowse
            case .settings: .tabSettings
            }
        return asset.withRenderingMode(.alwaysTemplate)
    }

    var title: String {
        switch self {
        case .chat: String(localized: .tabChat)
        case .wallet: String(localized: .tabWallet)
        case .browse: String(localized: .tabBrowse)
        case .settings: String(localized: .tabSettings)
        }
    }
}

extension TabBarItem {
    func makeBarItem(showsLabel: Bool, showsAttention: Bool) -> TabBarBarItem {
        TabBarBarItem(
            icon: image,
            title: showsLabel ? title : nil,
            accessibilityLabel: title,
            accessibilityID: AccessibilityID.Tab.item(for: self),
            accessibilityValue: showsAttention ? String(localized: .tabSettingsBackupNeeded) : nil,
            showsAttention: showsAttention
        )
    }
}

@MainActor
protocol SPAHosting: AnyObject {
    func openProduct(page: ProductPage)
    func minimizeSPA()
    func closeSPA(tabId: UUID)
}
