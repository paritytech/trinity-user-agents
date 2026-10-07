import Foundation
import Products
import Testing
import UIKit
import UIKitExt
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

        let first = hosts.view(for: loyalty, make: factory.make)
        let second = hosts.view(for: loyalty, make: factory.make)

        #expect(factory.built == 1)
        #expect(first === second)
    }

    /// One at a time: a second card takes the first down rather than adding to it.
    @Test
    func openingAnotherCardTakesTheFirstDown() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()

        let first = hosts.view(for: loyalty, make: factory.make)
        _ = hosts.view(for: trophy, make: factory.make)
        let loyaltyAgain = hosts.view(for: loyalty, make: factory.make)

        #expect(factory.built == 3)
        #expect(first !== loyaltyAgain)
    }

    /// A card the collection no longer holds has no next tap, so the product
    /// behind it is given up rather than kept warm.
    @Test
    func givesUpAProductWhoseCardIsGone() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        _ = hosts.view(for: loyalty, make: factory.make)

        hosts.keepOnly { $0 != loyalty }
        _ = hosts.view(for: loyalty, make: factory.make)

        #expect(factory.built == 2)
    }

    /// The product is wired to the runtime provider of the session that opened
    /// it, so one kept across a sign-out would be handed to the next session
    /// still talking to the last one's core.
    @Test
    func buildsAgainAfterTheSessionLetItGo() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        _ = hosts.view(for: loyalty, make: factory.make)

        hosts.release()
        _ = hosts.view(for: loyalty, make: factory.make)

        #expect(factory.built == 2)
    }

    @Test
    func keepsAProductWhoseCardIsStillHeld() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()
        _ = hosts.view(for: loyalty, make: factory.make)

        hosts.keepOnly { $0 == loyalty }
        _ = hosts.view(for: loyalty, make: factory.make)

        #expect(factory.built == 1)
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
private let trophy = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "trophy"))

@MainActor
private final class CountingFactory {
    private(set) var built = 0

    func make() -> SPAViewProtocol? {
        built += 1
        return StubSPAView()
    }
}
