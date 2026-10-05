import ExternalAccessibility
import PolkadotUI
import UIKit
import SnapKit

@available(iOS 26.0, *)
@MainActor
final class LiquidGlassChatNavigationBarController: ChatNavigationBarControlling {
    private weak var navigationItem: UINavigationItem?
    private let titleView: ChatHeaderView
    private let onStartCall: (ChatCallType) -> Void

    private var headerConfiguration: ChatHeaderConfiguration?
    private var callActions: [ChatCallType] = []
    private var contactMenu: UIMenu?

    init(
        navigationItem: UINavigationItem,
        titleView: ChatHeaderView,
        onStartCall: @escaping (ChatCallType) -> Void
    ) {
        self.navigationItem = navigationItem
        self.titleView = titleView
        self.onStartCall = onStartCall
    }

    func configure() {
        applyClearBackground()
        navigationItem?.style = .editor
        navigationItem?.leftItemsSupplementBackButton = true

        let avatarItem = UIBarButtonItem(customView: titleView)
        avatarItem.hidesSharedBackground = true

        navigationItem?.leftBarButtonItem = avatarItem
        applyHeader()
    }

    func apply(callActions: [ChatCallType]) {
        self.callActions = callActions
        refreshRightBarButtonItems()
    }

    func apply(contactMenu: UIMenu?) {
        self.contactMenu = contactMenu
        refreshContactMenuPresentation()
    }

    func update(headerConfiguration: ChatHeaderConfiguration) {
        self.headerConfiguration = headerConfiguration
        applyHeader()
    }
}

@available(iOS 26.0, *)
private extension LiquidGlassChatNavigationBarController {
    func applyClearBackground() {
        let standard = UINavigationBarAppearance()
        standard.configureWithDefaultBackground()
        standard.titleTextAttributes = Self.titleTextAttributes

        let scrollEdge = UINavigationBarAppearance()
        scrollEdge.configureWithTransparentBackground()
        scrollEdge.titleTextAttributes = Self.titleTextAttributes

        navigationItem?.standardAppearance = standard
        navigationItem?.scrollEdgeAppearance = scrollEdge
    }

    func applyHeader() {
        navigationItem?.title = headerConfiguration?.username
        navigationItem?.subtitle = headerConfiguration?.additionalInfo
    }

    static var titleTextAttributes: [NSAttributedString.Key: Any] {
        [.font: UIFont.titleLarge, .foregroundColor: UIColor.fgPrimary]
    }

    func refreshContactMenuPresentation() {
        navigationItem?.titleMenuProvider = contactMenu.map { menu in { _ in menu } }
        refreshRightBarButtonItems()
    }

    func refreshRightBarButtonItems() {
        let items = ChatCallBarButtons.make(for: callActions, onStartCall: onStartCall)
        navigationItem?.rightBarButtonItems = items.isEmpty ? nil : items
    }
}
