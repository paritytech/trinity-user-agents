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

    func makeBarItem(badge: DSTabBarItem.Badge?, spaTabCount: Int, showsLabel: Bool) -> DSTabBarItem {
        switch self {
        case let .tab(item):
            item.makeBarItem(badge: badge, showsLabel: showsLabel)
        case let .action(action):
            action.makeBarItem(spaTabCount: spaTabCount, showsLabel: showsLabel)
        }
    }
}

extension TabBarAction {
    func makeBarItem(spaTabCount: Int, showsLabel: Bool) -> DSTabBarItem {
        DSTabBarItem(
            content: content(spaTabCount: spaTabCount),
            title: showsLabel ? title : nil,
            role: .action,
            accessibilityLabel: accessibilityLabel(spaTabCount: spaTabCount),
            showsGlassBackground: showsGlassBackground
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

    var showsGlassBackground: Bool {
        switch self {
        case .scan: true
        default: false
        }
    }

    func content(spaTabCount: Int) -> DSTabBarItem.Content {
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
