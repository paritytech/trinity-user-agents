import struct TrUAPIHost.ProductId
import Products
import Testing
import UIKit
@testable import polkadot_app

/// A card's screen lives on the Wallet tab's stack, directly above the Pocket,
/// so the tab bar stays over it and a tab switch leaves it open, as on Android.
@MainActor
struct PocketCardNavigationTests {
    /// A link opening a card while another Wallet screen was showing puts the
    /// card straight above the Pocket, which is where its close flies back to.
    /// The card's page may ask for its face to move only once its screen is
    /// up, so the screen is reported shown only once it is on the stack.
    @Test
    func placesTheCardDirectlyAboveThePocket() {
        let pocket = UIViewController()
        let navigation = UINavigationController(rootViewController: pocket)
        navigation.pushViewController(UIViewController(), animated: false)
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView())
        var onDisplayWhenShown = false

        PocketCardNavigation.show(screen, on: navigation, animated: false) {
            onDisplayWhenShown = screen.isOnDisplay
        }

        #expect(navigation.viewControllers == [pocket, screen])
        #expect(onDisplayWhenShown)
    }

    /// A link may swap the open card for another. The card leaving must not
    /// take the Pocket back from the one that replaced it, or the Pocket would
    /// draw the new card as closed under its own screen.
    @Test
    func givesThePocketItsCardBackUnlessAnotherCardTookOver() {
        let closed = FakePocketOrigin(slot: nil)
        closed.setOpenItem(.card(loyaltyCard.key))
        closed.moveOpenItem(by: CGSize(width: 0, height: -300), clearingPocket: true, animated: false)
        let swapped = FakePocketOrigin(slot: nil)
        swapped.setOpenItem(.card(trophyCard.key))

        for origin in [closed, swapped] {
            PocketCardScreenViewController(card: loyaltyCard, product: StubSPAView(), origin: origin)
                .showCardInPocket()
        }

        #expect([closed.placement, swapped.placement] == [
            PocketPlacement(),
            PocketPlacement(openItem: .card(trophyCard.key))
        ])
    }
}

private let trophyCard = PocketCardViewModel(
    key: PocketCardKey(productId: try! ProductId("game.paseo"), cardId: PocketCardId(value: "trophy")),
    title: "Trophy",
    privileged: false,
    face: nil
)
