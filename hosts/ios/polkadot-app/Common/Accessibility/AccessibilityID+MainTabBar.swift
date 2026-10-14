import ExternalAccessibility

extension AccessibilityID.Tab {
    static func item(for tab: TabBarItem) -> (any AccessibilityIdentifying)? {
        switch tab {
        case .chat:
            chat
        case .wallet:
            wallet
        case .settings:
            settings
        default:
            nil
        }
    }
}
