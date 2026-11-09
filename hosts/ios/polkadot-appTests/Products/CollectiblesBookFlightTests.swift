import Testing
import UIKit
@testable import polkadot_app

/// A Collectibles screen flies its book up from the Pocket's peek on a push, and back down on a pop. A screen with no
/// peek to fly from or to fades instead.
@MainActor
struct CollectiblesBookFlightTests {
    @Test
    func fliesWithOrWithoutAnOrigin() {
        let navigation = AppNavigationController(rootViewController: UIViewController())
        let origin = FakePocketOrigin(slot: CGRect(x: 16, y: 420, width: 361, height: 536))
        let flying = CollectiblesViewController(product: StubSPAView(), origin: origin)
        let originless = CollectiblesViewController(product: StubSPAView())

        let pushes = [flying, originless].map { screen in
            navigation.navigationController(navigation, animationControllerFor: .push, from: navigation, to: screen)
        }
        let pops = [flying, originless].map { screen in
            navigation.navigationController(navigation, animationControllerFor: .pop, from: screen, to: navigation)
        }

        #expect(pushes.map { $0 is CollectiblesBookFlight } == [true, true])
        #expect(pops.map { $0 is CollectiblesBookReturn } == [true, true])
        withExtendedLifetime(origin) {}
    }
}
