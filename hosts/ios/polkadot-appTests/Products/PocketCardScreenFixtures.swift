import struct TrUAPIHost.ProductId
import Products
import Testing
import UIKit
@testable import polkadot_app

let loyaltyCard = PocketCardViewModel(
    key: PocketCardKey(productId: try! ProductId("game.paseo"), cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false,
    face: nil
)

/// Where the Pocket shows the card, in window coordinates.
let pocketSlot = CGRect(x: 24, y: 420, width: 345, height: PocketCardSize.height)

/// How the Pocket draws its open item: which item it is, how far it is moved
/// from its slot, and whether the rest of the Pocket has cleared away for it.
struct PocketPlacement: Equatable {
    var openItem: PocketItem?
    var offset: CGSize = .zero
    var cleared = false
}

/// A Pocket that shows every item in the same slot, and records its placement.
@MainActor
final class FakePocketOrigin: PocketOrigin {
    let slot: CGRect?
    private(set) var placement = PocketPlacement()

    init(slot: CGRect?) {
        self.slot = slot
    }

    func frame(of _: PocketItem) -> CGRect? {
        slot
    }

    var openItem: PocketItem? {
        placement.openItem
    }

    func setOpenItem(_ item: PocketItem?) {
        placement.openItem = item
    }

    func moveOpenItem(by offset: CGSize, clearingPocket: Bool, animated _: Bool) {
        placement.offset = offset
        placement.cleared = clearingPocket
    }
}
