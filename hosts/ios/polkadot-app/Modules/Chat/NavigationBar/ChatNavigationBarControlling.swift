import ExternalAccessibility
import PolkadotUI
import UIKit

@MainActor
protocol ChatNavigationBarControlling: AnyObject {
    func configure()
    func apply(callActions: [ChatCallType])
    func apply(contactMenu: UIMenu?)
    func update(headerConfiguration: ChatHeaderConfiguration)
}

enum ChatNavigationBarControllerFactory {
    @MainActor
    static func make(
        navigationItem: UINavigationItem,
        titleView: ChatHeaderView,
        onStartCall: @escaping (ChatCallType) -> Void
    ) -> ChatNavigationBarControlling {
        if #available(iOS 26.0, *) {
            LiquidGlassChatNavigationBarController(
                navigationItem: navigationItem,
                titleView: titleView,
                onStartCall: onStartCall
            )
        } else {
            LegacyChatNavigationBarController(
                navigationItem: navigationItem,
                titleView: titleView,
                onStartCall: onStartCall
            )
        }
    }
}

// Trailing call buttons (video + audio) — identical across iOS versions.
enum ChatCallBarButtons {
    @MainActor
    static func make(
        for callActions: [ChatCallType],
        onStartCall: @escaping (ChatCallType) -> Void
    ) -> [UIBarButtonItem] {
        var items: [UIBarButtonItem] = []
        if callActions.contains(.audio) { items.append(item(.audio, onStartCall)) }
        if callActions.contains(.video) { items.append(item(.video, onStartCall)) }
        return items
    }

    @MainActor
    private static func item(
        _ callType: ChatCallType,
        _ onStartCall: @escaping (ChatCallType) -> Void
    ) -> UIBarButtonItem {
        let image = UIImage(resource: callType == .video ? .icon28Video : .icon28PhoneCall)
        let title = String(localized: callType == .video ? .videoCall : .audioCall)
        let action = UIAction(title: title, image: image) { _ in onStartCall(callType) }
        let item = UIBarButtonItem(primaryAction: action)
        item.tintColor = .fgPrimary
        if callType == .audio {
            item.accessibilityId(AccessibilityID.Chat.voiceCallButton)
        }
        return item
    }
}
