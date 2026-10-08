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

    /// A screen claims its product's surface when it is built, so a second tap
    /// landing while the first open still waits to present would build a screen
    /// that takes the surface, and the page, from the one the user is shown.
    @Test
    func ignoresAnOpenWhileAnotherIsStillUnderWay() async {
        let hosts = PocketCardHosts()
        let opens = PresentedOpens()
        let (started, signalStarted) = AsyncStream<Void>.makeStream()
        let (lookedUp, signalLookedUp) = AsyncStream<Void>.makeStream()

        let first = Task {
            await hosts.openIfIdle {
                signalStarted.yield()
                for await _ in lookedUp {
                    break
                }
            } then: {
                opens.presented.append("first")
            }
        }
        for await _ in started {
            break
        }
        await hosts.openIfIdle {} then: { opens.presented.append("second") }
        signalLookedUp.yield()
        await first.value
        await hosts.openIfIdle {} then: { opens.presented.append("after") }

        #expect(opens.presented == ["first", "after"])
    }

    /// An open waits on its card's manifest before it builds anything, and the
    /// product it would build is wired to the session that started it. One the
    /// session ended under would hand the next session a page talking to the
    /// last one's core.
    @Test
    func dropsAnOpenTheSessionEndedUnder() async {
        let hosts = PocketCardHosts()
        let opens = PresentedOpens()
        let (started, signalStarted) = AsyncStream<Void>.makeStream()
        let (lookedUp, signalLookedUp) = AsyncStream<Void>.makeStream()

        let pending = Task {
            await hosts.openIfIdle {
                signalStarted.yield()
                for await _ in lookedUp {
                    break
                }
            } then: {
                opens.presented.append("pending")
            }
        }
        for await _ in started {
            break
        }
        hosts.release()
        signalLookedUp.yield()
        await pending.value
        await hosts.openIfIdle {} then: { opens.presented.append("after") }

        #expect(opens.presented == ["after"])
    }

    /// Letting go of a card the collection no longer holds is not the session
    /// ending: the card the user is opening meanwhile must still open.
    @Test
    func keepsAnOpenWhenTheCollectionDropsAnotherCard() async {
        let hosts = PocketCardHosts()
        let opens = PresentedOpens()
        let (started, signalStarted) = AsyncStream<Void>.makeStream()
        let (lookedUp, signalLookedUp) = AsyncStream<Void>.makeStream()
        _ = hosts.product(for: trophy) { _ in StubSPAView() }

        let pending = Task {
            await hosts.openIfIdle {
                signalStarted.yield()
                for await _ in lookedUp {
                    break
                }
            } then: {
                opens.presented.append("pending")
            }
        }
        for await _ in started {
            break
        }
        hosts.keepOnly { _ in false }
        signalLookedUp.yield()
        await pending.value

        #expect(opens.presented == ["pending"])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
private let trophy = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "trophy"))

@MainActor
private final class PresentedOpens {
    var presented: [String] = []
}

@MainActor
private final class CountingFactory {
    private(set) var surfaces: [PocketCardSurface] = []
    var built: Int { surfaces.count }

    func make(surface: PocketCardSurface) -> SPAViewProtocol? {
        surfaces.append(surface)
        return StubSPAView()
    }
}
