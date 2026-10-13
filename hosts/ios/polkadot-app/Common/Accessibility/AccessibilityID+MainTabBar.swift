import ExternalAccessibility

extension AccessibilityID.Tab {
    static func item(for tab: TabBarItem) -> AccessibilityID.Tab? {
        switch tab {
        case .chat:
            chat
        case .wallet:
            wallet
        default:
            nil
        }
    }
}
