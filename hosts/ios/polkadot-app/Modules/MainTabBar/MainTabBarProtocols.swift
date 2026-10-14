import UIKit
import PolkadotUI
import UIKitExt
import ExternalAccessibility
import Products

protocol MainTabBarViewProtocol: ControllerBackedProtocol, AppWidgetManaging {
    func show(slots: [TabBarSlot], selecting tab: TabBarItem)
    func select(tab: TabBarItem)
    func setBadge(_ badge: TabBarBadge?, for tab: TabBarItem)
    func setLabels(visible: Bool)
    func showSPATabs(_ viewModels: [SPATabChipViewModel])
    func showTabBarPanelContent(_ configuration: (any HashableContentConfiguration)?, for action: TabBarAction)
    func showScanPanel()
    func showChainStatus(_ rows: [ChainConnectionStatusViewModel])
    func showProductGamePill(_ viewModel: ProductGamePillViewModel?)
}

@MainActor
protocol MainTabBarPresenterProtocol: AnyObject {
    func setup()
    func configureViews()
    func didRequestContentPanel(for action: TabBarAction)
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
    func didUpdateSettingsAttention(isVisible: Bool)
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

enum TabBarBadge: Equatable {
    case attention
}

extension TabBarItem {
    func makeBarItem(badge: DSTabBarItem.Badge?, showsLabel: Bool) -> DSTabBarItem {
        DSTabBarItem(
            icon: image,
            title: showsLabel ? title : nil,
            badge: badge,
            accessibilityLabel: title,
            accessibilityIdentifier: AccessibilityID.Tab.item(for: self)?.rawValue
        )
    }
}

@MainActor
protocol SPAHosting: AnyObject {
    func openProduct(page: ProductPage)
    func minimizeSPA()
    func closeSPA(tabId: UUID)
}
