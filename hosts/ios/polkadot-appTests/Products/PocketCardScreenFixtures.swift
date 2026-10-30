import Products
import Testing
import UIKit
@testable import polkadot_app

let loyaltyCard = PocketCardViewModel(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false,
    face: nil
)

/// Where the Pocket shows the card, in window coordinates.
let pocketSlot = CGRect(x: 24, y: 420, width: 345, height: PocketCardSize.height)

/// How the Pocket draws its open card: which card it is, how far it is moved
/// from its slot, and whether the rest of the Pocket has cleared away for it.
struct PocketPlacement: Equatable {
    var openCard: PocketCardKey?
    var offset: CGSize = .zero
    var cleared = false
}

@MainActor
final class FakePocketCardOrigin: PocketCardOrigin {
    let slot: CGRect?
    private(set) var placement = PocketPlacement()

    init(slot: CGRect?) {
        self.slot = slot
    }

    func frame(of _: PocketCardKey) -> CGRect? {
        slot
    }

    var openCard: PocketCardKey? {
        placement.openCard
    }

    func setOpenCard(_ key: PocketCardKey?) {
        placement.openCard = key
    }

    func moveOpenCard(by offset: CGSize, clearingPocket: Bool, animated _: Bool) {
        placement.offset = offset
        placement.cleared = clearingPocket
    }
}
