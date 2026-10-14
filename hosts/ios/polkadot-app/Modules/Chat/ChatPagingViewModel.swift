import Foundation
import PolkadotUI

extension ChatViewLayout.ViewModel.Paging {
    /// The rows at the ends of the window: a spinner while a page loads, a text when it failed.
    init(paging: ChatPagingState) {
        self.init(older: Edge(edge: paging.older), newer: Edge(edge: paging.newer))
    }
}

private extension ChatViewLayout.ViewModel.Paging.Edge {
    init(edge: ChatPagingState.Edge) {
        switch edge {
        case .more:
            self = .more
        case .loading:
            self = .loading
        case .end:
            self = .end
        case .failed:
            self = .failed(String(localized: .ChatExtension.pageFailed))
        }
    }
}
