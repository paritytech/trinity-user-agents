import struct TrUAPIHost.ProductId
import Foundation
import Products
import Testing
@testable import polkadot_app

/// A web view and a page load are what a card costs to open, so a card opened
/// again is worth not paying twice. One is held at a time, which is the bound
/// that keeps a Pocket full of cards from becoming a web view each.
@MainActor
struct PocketCardHostsTests {
    @Test
    func opensACardOnceAndReusesItAfterwards() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()

        let first = hosts.product(for: loyalty, make: factory.make)
        let second = hosts.product(for: loyalty, make: factory.make)

        #expect(factory.built == 1)
        #expect(first?.view === second?.view)
    }

    /// A page is loaded with the params of the link that opened it, so a link
    /// with other params, or a tap with none, has to load the page again.
    @Test
    func reusesAPageOnlyForTheParamsItWasOpenedWith() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()

        let linked = hosts.product(for: loyalty, pageQuery: "open=scratch", make: factory.make)
        let linkedAgain = hosts.product(for: loyalty, pageQuery: "open=scratch", make: factory.make)
        let tapped = hosts.product(for: loyalty, make: factory.make)

        #expect(factory.built == 2)
        #expect(linked?.view === linkedAgain?.view)
        #expect(tapped?.view !== linked?.view)
    }

    /// A link to the card on screen cannot open a second screen, so its params
    /// are loaded into the page already there. The same params leave the page
    /// where the user took it.
    @Test
    func loadsNewParamsIntoTheHeldPage() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        let held = hosts.product(for: loyalty, pageQuery: "open=scratch", make: factory.make)
        var loaded: [String] = []

        hosts.reload(loyalty, at: "open=scratch") { _ in loaded.append("open=scratch") }
        hosts.reload(loyalty, at: "open=prizes") { _ in loaded.append("open=prizes") }
        hosts.reload(trophy, at: "open=prizes") { _ in loaded.append("trophy") }
        let reopened = hosts.product(for: loyalty, pageQuery: "open=prizes", make: factory.make)

        #expect(loaded == ["open=prizes"])
        #expect(factory.built == 1)
        #expect(reopened?.view === held?.view)
    }

    /// One at a time: a second card takes the first down rather than adding to it.
    @Test
    func openingAnotherCardTakesTheFirstDown() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()

        let first = hosts.product(for: loyalty, make: factory.make)
        _ = hosts.product(for: trophy, make: factory.make)
        let loyaltyAgain = hosts.product(for: loyalty, make: factory.make)

        #expect(factory.built == 3)
        #expect(first?.view !== loyaltyAgain?.view)
    }

    @Test
    func isOnDisplayOnlyWhileTheHeldCardsScreenIs() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        _ = hosts.product(for: loyalty, make: factory.make)

        #expect(hosts.isOnDisplay(loyalty))
        #expect(!hosts.isOnDisplay(trophy))

        factory.views.first?.isOnDisplay = false

        #expect(!hosts.isOnDisplay(loyalty))
    }

    /// A card the collection no longer holds has no next tap, so the product
    /// behind it is given up rather than kept warm.
    @Test
    func givesUpAProductWhoseCardIsGone() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        _ = hosts.product(for: loyalty, make: factory.make)

        hosts.keepOnly { $0 != loyalty }
        _ = hosts.product(for: loyalty, make: factory.make)

        #expect(factory.built == 2)
    }

    /// The product is wired to the runtime provider of the session that opened
    /// it, so one kept across a sign-out would be handed to the next session
    /// still talking to the last one's core.
    @Test
    func buildsAgainAfterTheSessionLetItGo() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        _ = hosts.product(for: loyalty, make: factory.make)

        hosts.release()
        _ = hosts.product(for: loyalty, make: factory.make)

        #expect(factory.built == 2)
    }

    @Test
    func keepsAProductWhoseCardIsStillHeld() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        _ = hosts.product(for: loyalty, make: factory.make)

        hosts.keepOnly { $0 == loyalty }
        _ = hosts.product(for: loyalty, make: factory.make)

        #expect(factory.built == 1)
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: try! ProductId("game.paseo"), cardId: PocketCardId(value: "loyalty"))
private let trophy = PocketCardKey(productId: try! ProductId("game.paseo"), cardId: PocketCardId(value: "trophy"))

@MainActor
private final class CountingFactory {
    private(set) var views: [StubExpandedPocketCardView] = []
    var built: Int { views.count }

    func make() -> ExpandedPocketCardViewProtocol? {
        let view = StubExpandedPocketCardView()
        views.append(view)
        return view
    }
}
