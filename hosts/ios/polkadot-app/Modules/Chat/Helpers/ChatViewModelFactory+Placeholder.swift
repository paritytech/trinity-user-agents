import Foundation

extension MessageListModel {
    /// The room shows its placeholder only when the window holds the whole room: a product-source room that
    /// still loads its first page, or that did not answer, shows no placeholder (P6.8, 02-DESIGN §8.6).
    var showsPlaceholder: Bool {
        paging.older == .end && paging.newer == .end && readiness == .ready
    }
}
