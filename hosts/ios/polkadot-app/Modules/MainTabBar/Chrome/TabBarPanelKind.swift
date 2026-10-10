enum TabBarPanelKind: Equatable {
    case spaTabs
    /// The product shelf that grows above the bar row from the Apps item.
    case shelf
    case content(TabBarAction)

    /// The bar action whose item opens this panel.
    var action: TabBarAction {
        switch self {
        case .spaTabs,
             .shelf:
            .spaTabs
        case let .content(action):
            action
        }
    }

    /// Non-nil only for panels that host content, which `.spaTabs` and `.shelf` do not.
    var contentAction: TabBarAction? {
        guard case let .content(action) = self else {
            return nil
        }
        return action
    }
}
