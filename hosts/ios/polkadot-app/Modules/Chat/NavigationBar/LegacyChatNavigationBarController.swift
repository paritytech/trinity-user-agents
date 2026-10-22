import ExternalAccessibility
import PolkadotUI
import SnapKit
import UIKit

@MainActor
final class LegacyChatNavigationBarController: ChatNavigationBarControlling {
    private weak var navigationItem: UINavigationItem?
    private let titleView: ChatHeaderView
    private let onStartCall: (ChatCallType) -> Void

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
        let items = ChatCallBarButtons.make(for: callActions, onStartCall: onStartCall)
        navigationItem?.rightBarButtonItems = items.isEmpty ? nil : items
    }

    func apply(contactMenu: UIMenu?) {
        menuButton.menu = contactMenu
        menuButton.isUserInteractionEnabled = contactMenu != nil
    }

    func update(headerConfiguration _: ChatHeaderConfiguration) {}
}
