import Foundation
import PolkadotUI

extension ChatViewLayout.ViewModel.Paging {
    /// What the chat can ask at each end of the window. The chat draws no row for a page, as the native chat draws
    /// none: a page that failed is asked again when the user scrolls near its end.
    init(paging: ChatPagingState) {
        self.init(older: Edge(edge: paging.older), newer: Edge(edge: paging.newer))
    }
}

private extension ChatViewLayout.ViewModel.Paging.Edge {
    init(edge: ChatPagingState.Edge) {
        switch edge {
        case .more,
             .failed:
            self = .more
        case .loading:
            self = .loading
        case .end:
            self = .end
        }
    }
}
