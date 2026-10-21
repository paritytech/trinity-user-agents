enum TabBarPanelKind: Equatable {
    /// The shelf that grows above the bar row from the Apps item, or from the Network item in builds
    /// without products.
    case shelf
    case content(TabBarAction)

    /// The bar action whose item opens this panel.
    var action: TabBarAction {
        switch self {
        case .shelf:
            #if FEATURE_PRODUCTS
                .spaTabs
            #else
                .connectionStatus
            #endif
        case let .content(action):
            action
        }
    }
}
