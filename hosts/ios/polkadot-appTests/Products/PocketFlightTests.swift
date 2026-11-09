import Products
import Testing
import UIKit
@testable import polkadot_app

/// A card opened from the Pocket flies from its place there to the top of its
/// screen, and back when closed, so the user sees one card move.
@MainActor
struct PocketFlightTests {
    /// Every card screen takes a flight on push and on pop, with or without a
    /// Pocket to fly from, so no card opens or closes with the system's slide.
    /// Every other screen keeps the system's transition.
    @Test
    func animatesEveryCardScreenWithAFlight() {
        let navigation = AppNavigationController(rootViewController: UIViewController())
        let origin = FakePocketOrigin(slot: pocketSlot)
        let flying = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
        let pocketless = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        let screens = [flying, pocketless, UIViewController()]

        let pushes = screens.map { screen in
            navigation.navigationController(navigation, animationControllerFor: .push, from: navigation, to: screen)
        }
        let pops = screens.map { screen in
            navigation.navigationController(navigation, animationControllerFor: .pop, from: screen, to: navigation)
        }

        #expect(pushes.map { $0 is PocketFlight } == [true, true, false])
        #expect(pops.map { $0 is PocketFlight } == [true, true, false])
        withExtendedLifetime(origin) {}
    }

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
