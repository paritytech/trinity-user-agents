import UIKit

/// An item the Pocket opens onto a screen of its own.
enum PocketItem: Equatable {
    case balance
    case collectibles
    case card(PocketCardKey)
}

/// The Pocket an item is opened from: where the item sits in it, and how it
/// draws that item while the item's screen is up.
@MainActor
protocol PocketOrigin: AnyObject {
    /// The item's frame in window coordinates, or nil when the Pocket cannot
    /// show the item where the user would see it land.
    func frame(of item: PocketItem) -> CGRect?

    /// The item whose screen is up, or nil once it has closed.
    var openItem: PocketItem? { get }

    func setOpenItem(_ item: PocketItem?)

    /// Draws the open item `offset` away from its slot, with the rest of the
    /// Pocket cleared away from it by `clearance`: 0 at rest, 1 cleared. A
    /// duration animates the change on the flight's curve; nil applies it at once.
    func moveOpenItem(by offset: CGSize, clearance: CGFloat, animatedOver duration: TimeInterval?)
}

extension PocketOrigin {
    /// Clears the Pocket fully or brings it back, on the flight's clock when animated.
    func moveOpenItem(by offset: CGSize, clearingPocket: Bool, animated: Bool) {
        moveOpenItem(
            by: offset,
            clearance: clearingPocket ? 1 : 0,
            animatedOver: animated ? PocketFlightTiming.duration : nil
        )
    }
}

/// A screen that shows an item the Pocket opened.
@MainActor
protocol PocketItemScreen: UIViewController {
    var pocketItem: PocketItem { get }

    /// The title the status strip shows while this screen is up.
    var statusStripTitle: String { get }
}
