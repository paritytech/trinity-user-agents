import ExternalAccessibility
import PolkadotUI
import SnapKit
import UIKit

@MainActor
final class LegacyChatNavigationBarController: ChatNavigationBarControlling {
    private weak var navigationItem: UINavigationItem?
    private let titleView: ChatHeaderView
    private let onStartCall: (ChatCallType) -> Void

    private var callItems: [UIBarButtonItem] = []
    private var productToolbar: [UIBarButtonItem] = []
    private var searchItem: UIBarButtonItem?

    private lazy var menuButton: UIButton = {
        let button = UIButton(type: .custom)
        button.showsMenuAsPrimaryAction = true
        return button
    }()

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
        titleView.translatesAutoresizingMaskIntoConstraints = false
        navigationItem?.titleView = titleView
        titleView.addSubview(menuButton)
        menuButton.snp.makeConstraints { $0.edges.equalToSuperview() }
    }

    func apply(callActions: [ChatCallType]) {
        callItems = ChatCallBarButtons.make(for: callActions, onStartCall: onStartCall)
        refreshRightBarButtonItems()
    }

    func apply(productToolbar: [UIBarButtonItem]) {
        self.productToolbar = productToolbar
        refreshRightBarButtonItems()
    }

    func apply(searchItem: UIBarButtonItem?) {
        self.searchItem = searchItem
        refreshRightBarButtonItems()
    }

    private func refreshRightBarButtonItems() {
        let items = productToolbar + [searchItem].compactMap { $0 } + callItems
        navigationItem?.rightBarButtonItems = items.isEmpty ? nil : items
    }

    func apply(contactMenu: UIMenu?) {
        menuButton.menu = contactMenu
        menuButton.isUserInteractionEnabled = contactMenu != nil
    }

    func update(headerConfiguration _: ChatHeaderConfiguration) {}
}
