import Testing
import UIKit
@testable import polkadot_app

/// A Collectibles screen flies its book up from the Pocket's peek on a push. A pop, and a push with no peek to fly
/// from, keep the system transition.
@MainActor
struct CollectiblesBookFlightTests {
    @Test
    func fliesOnlyAPushOntoAScreenWithAnOrigin() {
        let navigation = AppNavigationController(rootViewController: UIViewController())
        let origin = FakeCollectiblesOrigin()
        let flying = CollectiblesViewController(product: StubSPAView(), origin: origin)
        let originless = CollectiblesViewController(product: StubSPAView())

        let pushes = [flying, originless].map { screen in
            navigation.navigationController(navigation, animationControllerFor: .push, from: navigation, to: screen)
        }
        let pop = navigation.navigationController(
            navigation,
            animationControllerFor: .pop,
            from: flying,
            to: navigation
        )

        #expect(pushes.map { $0 is CollectiblesBookFlight } == [true, false])
        #expect(pop == nil)
        withExtendedLifetime(origin) {}
    }
}
