import Products
import Testing
import UIKit
@testable import polkadot_app

/// A card opened from the Pocket flies from its place there to the top of its
/// screen, and back when closed, so the user sees one card move.
@MainActor
struct PocketCardFlightTests {
    /// Only a card's screen flies. Every other screen on a tab keeps the
    /// system's push, and so does a card with no Pocket to fly from.
    @Test
    func fliesOnlyACardThatHasAPocketToFlyFrom() {
        let navigation = AppNavigationController(rootViewController: UIViewController())
        let origin = FakePocketCardOrigin(slot: pocketSlot)
        let flying = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
        let pocketless = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())

        let animators = [flying, pocketless, UIViewController()].map { screen in
            navigation.navigationController(navigation, animationControllerFor: .push, from: navigation, to: screen)
        }

        #expect(animators.map { $0 is PocketCardFlight } == [true, false, false])
        withExtendedLifetime(origin) {}
    }
}
