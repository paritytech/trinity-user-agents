enum TabBarPanelKind: Equatable {
    /// The shelf that grows above the bar row from the Apps item.
    case shelf
    case content(TabBarAction)

    /// The bar action whose item opens this panel.
    var action: TabBarAction {
        switch self {
        case .shelf:
            .spaTabs
        case let .content(action):
            action
        }
    }
}
