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

        let first = hosts.product(for: loyalty, make: factory.make)
        let second = hosts.product(for: loyalty, make: factory.make)

        #expect(factory.built == 1)
        #expect(first?.view === second?.view)
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

    /// The surface is how a warm page reaches whichever screen shows it next,
    /// so the card reopened must find the one its page was built with.
    @Test
    func keepsTheSurfaceAWarmProductWasBuiltWith() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()

        let first = hosts.product(for: loyalty, make: factory.make)
        let second = hosts.product(for: loyalty, make: factory.make)

        #expect(first?.surface === factory.surfaces.first)
        #expect(second?.surface === first?.surface)
    }

    /// Another card's page must never move this card's face.
    @Test
    func givesEachProductItsOwnSurface() {
        let hosts = PocketCardHosts()
        let factory = CountingFactory()

        let first = hosts.product(for: loyalty, make: factory.make)
        let second = hosts.product(for: trophy, make: factory.make)

        #expect(first?.surface !== second?.surface)
    }

    /// A card already open is what the user asked for. Opening it again would
    /// build a second screen that takes its page, and closing that one would
    /// leave the card under it blank.
    @Test
    func knowsWhetherACardIsOpenOnScreen() async throws {
        let hosts = PocketCardHosts()
        let product = try #require(hosts.product(for: loyalty) { _ in StubSPAView() })
        let screen = PocketCardScreenViewController(card: loyaltyCard, product: product.view)
        let presenter = UIViewController()
        let window = showing(presenter)
        let beforeOpening = hosts.isOnDisplay(loyalty)

        await present(cardNavigation(screen), from: presenter)
        product.surface.claim(screen)
        let whileOpen = [hosts.isOnDisplay(loyalty), hosts.isOnDisplay(trophy)]
        await dismissPresented(from: presenter)

        #expect([beforeOpening, hosts.isOnDisplay(loyalty)] == [false, false])
        #expect(whileOpen == [true, false])
        withExtendedLifetime(window) {}
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

private let loyalty = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
private let trophy = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "trophy"))

@MainActor
private final class CountingFactory {
    private(set) var surfaces: [PocketCardSurface] = []
    var built: Int { surfaces.count }

    func make(surface: PocketCardSurface) -> SPAViewProtocol? {
        surfaces.append(surface)
        return StubSPAView()
    }
}
