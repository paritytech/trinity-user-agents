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

    /// Draws the open item `offset` away from its slot, and clears the rest of
    /// the Pocket away from it or brings it back, on the flight's clock when
    /// animated.
    func moveOpenItem(by offset: CGSize, clearingPocket: Bool, animated: Bool)
}

/// A screen that shows an item the Pocket opened.
@MainActor
protocol PocketItemScreen: UIViewController {
    var pocketItem: PocketItem { get }
}
