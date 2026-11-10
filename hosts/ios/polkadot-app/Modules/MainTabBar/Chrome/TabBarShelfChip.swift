import Foundation
import PolkadotUI

/// One open product as the shelf shows it.
struct TabBarShelfChip: Identifiable, Equatable {
    let id: UUID
    let name: String
    let icon: ImageViewModelProtocol?

    /// The icon protocol has no equality, so icons are compared by object identity. A non-class icon boxes to a new
    /// object each time and so never compares equal, which only costs a redraw.
    static func == (lhs: TabBarShelfChip, rhs: TabBarShelfChip) -> Bool {
        lhs.id == rhs.id && lhs.name == rhs.name && (lhs.icon as AnyObject?) === (rhs.icon as AnyObject?)
    }
}
