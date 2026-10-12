import Products
import Testing
import UIKit
import UIKitExt
@testable import polkadot_app

/// A card opened is the card and its product on one screen, sharing one
/// scroll: the product gets a screenful, and the card is what there is to
/// scroll past to give the product all of it.
@MainActor
struct PocketCardScreenTests {
    @Test
    func givesTheProductAScreenfulWithTheCardAboveIt() {
        let screen = laidOutScreen(product: StubSPAView())

        let scrollView = screen.view.subviews.compactMap { $0 as? UIScrollView }.first

        #expect(scrollView?.contentSize.height == PocketOpenedCardView.height + screenSize.height)
    }

    /// A product page that fits its screenful has nothing to scroll, and its
    /// own rubber-banding would swallow the gesture meant for the card.
    @Test
    func leavesTheCardsGestureToTheScreenWhenThePageFits() {
        let product = StubSPAView()

        _ = laidOutScreen(product: product)

        #expect(product.pageScrollView.bounces == false)
    }

    /// The screen borrows a product that outlives it, so the title belongs to
    /// the card rather than to whatever the product later calls itself.
    @Test
    func takesItsTitleFromTheCard() {
        let screen = PocketCardScreenViewController(card: loyalty, product: StubSPAView())

        screen.loadViewIfNeeded()

        #expect(screen.title == "Loyalty")
    }

    /// The product is kept warm for the next tap on the same card, which only
    /// works if the screen it was shown on let go of it.
    @Test
    func handsTheProductBackFreeToShowAgain() {
        let product = StubSPAView()
        let screen = laidOutScreen(product: product)

        screen.handBackProduct()

        #expect(product.controller.parent == nil)
        #expect(product.controller.view.superview == nil)
    }
}

// MARK: - Fixtures

private let screenSize = CGSize(width: 393, height: 800)

private let loyalty = PocketCardViewModel(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false,
    face: nil
)

@MainActor
private func laidOutScreen(product: SPAViewProtocol) -> PocketCardScreenViewController {
    let screen = PocketCardScreenViewController(card: loyalty, product: product)

    screen.view.frame = CGRect(origin: .zero, size: screenSize)
    screen.view.layoutIfNeeded()

    return screen
}
