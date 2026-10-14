enum TabBarPanelKind: Equatable {
    case spaTabs
    case content(TabBarAction)

    /// The bar action whose item opens this panel.
    var action: TabBarAction {
        switch self {
        case .spaTabs:
            .spaTabs
        case let .content(action):
            action
        }
    }

    /// Non-nil only for panels that host content, which `.spaTabs` does not.
    var contentAction: TabBarAction? {
        guard case let .content(action) = self else {
            return nil
        }
        return action
    }
}
