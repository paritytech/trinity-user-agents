import Products
import Testing
import UIKit
@testable import polkadot_app

/// A card opened from the Pocket flies from its place there to the top of its
/// screen, and back when closed, so the user sees one card move.
@MainActor
struct PocketFlightTests {
    /// A card the Pocket shows flies to its slot, which is the path the user
    /// sees the card take.
    @Test
    func fliesToTheSlotWithoutReduceMotion() {
        let slot = CGPoint(x: 24, y: 420)

        #expect(PocketFlight.motion(toSlot: slot, reduceMotion: false) == .fly(toSlot: slot))
    }

    /// A card the Pocket cannot place has no slot to fly to, so it fades with
    /// the rest of its screen rather than sliding from nowhere.
    @Test
    func fadesWhenThePocketCannotPlaceTheCard() {
        #expect(PocketFlight.motion(toSlot: nil, reduceMotion: false) == .fade)
    }

    /// Reduce Motion crossfades the card's screen whether or not the card has a
    /// slot, since a card flying across the screen is the motion the setting asks
    /// to avoid.
    @Test
    func crossfadesUnderReduceMotionWhetherOrNotTheCardHasASlot() {
        #expect(PocketFlight.motion(toSlot: CGPoint(x: 24, y: 420), reduceMotion: true) == .crossfade)
        #expect(PocketFlight.motion(toSlot: nil, reduceMotion: true) == .crossfade)
    }
}
