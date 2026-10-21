import UIKit
import PolkadotUI

/// A non-tab affordance in the bar. Tapping one opens a panel instead of switching tabs.
enum TabBarAction: Hashable {
    case scan
    case spaTabs
    case connectionStatus
}

enum TabBarSlot: Equatable {
    case tab(TabBarItem)
    case action(TabBarAction)
}

extension TabBarSlot {
    var tab: TabBarItem? {
        guard case let .tab(item) = self else {
            return nil
        }
        return item
    }

    func makeBarItem(spaTabCount: Int, showsLabel: Bool, attentionTabs: Set<TabBarItem>) -> TabBarBarItem {
        switch self {
        case let .tab(item):
            item.makeBarItem(showsLabel: showsLabel, showsAttention: attentionTabs.contains(item))
        case let .action(action):
            action.makeBarItem(spaTabCount: spaTabCount, showsLabel: showsLabel)
        }
    }
}

extension TabBarAction {
    func makeBarItem(spaTabCount: Int, showsLabel: Bool) -> TabBarBarItem {
        TabBarBarItem(
            content: content(spaTabCount: spaTabCount),
            title: showsLabel ? title : nil,
            accessibilityLabel: accessibilityLabel(spaTabCount: spaTabCount)
        )
    }
}

private extension TabBarAction {
    var title: String? {
        switch self {
        case .scan: nil
        case .spaTabs: String(localized: .tabApps)
        case .connectionStatus: String(localized: .tabNetwork)
        }
    }

    func content(spaTabCount: Int) -> TabBarBarItem.Content {
        switch self {
        case .scan:
            .icon(UIImage.tabScan.withRenderingMode(.alwaysTemplate))
        case .spaTabs:
            .tabsGlyph(count: spaTabCount)
        case .connectionStatus:
            .icon(DSTabBarConnectionGlyph.image)
        }
    }

    func accessibilityLabel(spaTabCount: Int) -> String {
        switch self {
        case .scan:
            String(localized: .Products.productTabsAccessibilityScanner)
        case .spaTabs:
            String(localized: .Products.productTabsAccessibilityOpenApps(spaTabCount))
        case .connectionStatus:
            String(localized: .Common.connectionStatusAccessibilityLabel)
        }
    }
}
