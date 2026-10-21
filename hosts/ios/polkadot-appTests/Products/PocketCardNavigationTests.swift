import Products
import Testing
import UIKit
@testable import polkadot_app

/// A card's screen lives on the Wallet tab's stack, directly above the Pocket,
/// so the tab bar stays over it and a tab switch leaves it open, as on Android.
@MainActor
struct PocketCardNavigationTests {
    /// A card tapped in the Pocket on screen flies up from it.
    @Test
    func pushesACardOpenedFromThePocketOnScreen() async throws {
        let (navigation, window) = try walletStack()
        let pocket = try #require(navigation.viewControllers.first)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())

        PocketCardNavigation.show(screen, on: navigation, animated: true) {}
        let pushing = navigation.transitionCoordinator?.isAnimated == true
        await transitionFinished(in: navigation)

        #expect(pushing)
        #expect(navigation.viewControllers == [pocket, screen])
        withExtendedLifetime(window) {}
    }

    /// A link may name another card while one is open. The open card gives way
    /// to it, gives its product back and leaves the Pocket to the new card, or
    /// the Pocket would show the old one as open.
    @Test
    func swapsAnOpenCardForTheOneAskedFor() async throws {
        let (navigation, window) = try walletStack()
        let pocket = try #require(navigation.viewControllers.first)
        let origin = FakePocketCardOrigin(slot: nil)
        let firstProduct = StubSPAView()
        let first = PocketCardScreenViewController(card: loyaltyCard, product: firstProduct, origin: origin)
        let second = PocketCardScreenViewController(card: trophyCard, product: StubSPAView(), origin: origin)
        await openCard(first, on: navigation)

        PocketCardNavigation.show(second, on: navigation, animated: true) {}
        let handedBack = await eventually { firstProduct.controller.parent == nil }

        #expect(navigation.viewControllers == [pocket, second])
        #expect(handedBack)
        #expect(origin.openCard == trophyCard.key)
        withExtendedLifetime(window) {}
    }

    /// A link opening a card while another tab, or another Wallet screen, was
    /// showing puts the card straight above the Pocket. On Android the card is
    /// simply there when the Wallet shows.
    @Test
    func placesTheCardAboveThePocketWhenThePocketWasNotOnScreen() throws {
        let (navigation, window) = try walletStack()
        let pocket = try #require(navigation.viewControllers.first)
        navigation.pushViewController(UIViewController(), animated: false)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())

        PocketCardNavigation.show(screen, on: navigation, animated: false) {}

        #expect(navigation.viewControllers == [pocket, screen])
        #expect(navigation.transitionCoordinator == nil)
        withExtendedLifetime(window) {}
    }

    /// A link can arrive while a card is still flying. The stack takes no new
    /// screens mid-transition, so the card waits for it to finish.
    @Test
    func waitsForATransitionUnderWay() async throws {
        let (navigation, window) = try walletStack()
        let pocket = try #require(navigation.viewControllers.first)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        navigation.pushViewController(UIViewController(), animated: true)

        PocketCardNavigation.show(screen, on: navigation, animated: true) {}
        let shown = await eventually { navigation.viewControllers == [pocket, screen] }

        #expect(shown)
        withExtendedLifetime(window) {}
    }

    /// The card's page may ask for its face to move only once its screen is
    /// up, so the screen claims the page only once it is on the stack.
    @Test
    func reportsTheCardShownOnceItIsOnTheStack() async throws {
        let (navigation, window) = try walletStack()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        navigation.pushViewController(UIViewController(), animated: true)

        let onDisplayWhenShown = await withCheckedContinuation { shown in
            PocketCardNavigation
                .show(screen, on: navigation, animated: true) { shown.resume(returning: screen.isOnDisplay) }
        }

        #expect(onDisplayWhenShown)
        withExtendedLifetime(window) {}
    }

    /// A card on the Wallet tab can be closed while another tab is selected,
    /// with no appearance calls to say so. Its product must still come back
    /// and the Pocket get its card back.
    @Test
    func givesTheProductAndThePocketBackWhenClosedOffScreen() async throws {
        let (navigation, window) = try walletStack()
        let origin = FakePocketCardOrigin(slot: nil)
        let product = StubSPAView()
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product, origin: origin)
        await openCard(screen, on: navigation)

        window.rootViewController = UIViewController()
        navigation.popViewController(animated: false)

        #expect(product.controller.parent == nil)
        #expect(origin.placement == PocketPlacement())
        withExtendedLifetime(window) {}
    }
}

private let trophyCard = PocketCardViewModel(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "trophy")),
    title: "Trophy",
    privileged: false,
    face: nil
)
