import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A handler is what asks for a product's worker, so what starts one decides
/// which web views the session runs. A handler too many burns one for a product
/// that draws nothing; one too few leaves its cards on the face they were last
/// drawn with.
struct PocketHandlersTests {
    @Test
    func runsOneHandlerForAProductHoldingTheCardsItOffers() async throws {
        let workers = workersRunning("game.paseo")
        let handlers = makeHandlers(workers: workers)

        #expect(await handlers.reconcile([loyalty, streak]))

        #expect(handlers.handler(of: "game.paseo") != nil)
        #expect(workers.references.acquired == ["game.paseo"])
    }

    /// A product that offers no such card draws nothing, and a handler for it
    /// would boot a worker anyway. The host-placed card every Pocket carries
    /// sits on a product like that.
    @Test
    func runsNoHandlerForACardTheProductDoesNotOffer() async throws {
        let published = StubPublishedCards()
        published.failures = [PocketPublishError.noPocket]
        let workers = workersRunning("game.paseo")
        let handlers = makeHandlers(workers: workers, published: published)

        #expect(await handlers.reconcile([loyalty]))

        #expect(handlers.handler(of: "game.paseo") == nil)
        #expect(workers.references.acquired.isEmpty)
    }

    /// Looking a card up is a chain read. A read that did not land is not the
    /// product saying no, so it is reported as unsettled and asked again;
    /// treating it as a no would leave the card on its kept face until the
    /// collection next moves, which may be never.
    @Test
    func reportsALookupThatDidNotLandAsUnsettled() async throws {
        let published = StubPublishedCards()
        published.failures = [URLError(.notConnectedToInternet)]
        let workers = workersRunning("game.paseo")
        let handlers = makeHandlers(workers: workers, published: published)

        #expect(await !handlers.reconcile([loyalty]))
        #expect(handlers.handler(of: "game.paseo") == nil)

        #expect(await handlers.reconcile([loyalty]))
        #expect(handlers.handler(of: "game.paseo") != nil)
    }

    /// A read that did not land is not the product saying its card is gone.
    /// Stopping a handler on one would take down a card that is drawing, and
    /// the retry a moment later would boot the worker all over again.
    @Test
    func keepsAHandlerWhoseCardLookupDidNotLand() async throws {
        let published = StubPublishedCards()
        let workers = workersRunning("game.paseo")
        let handlers = makeHandlers(workers: workers, published: published)
        _ = await handlers.reconcile([loyalty])

        published.failures = [URLError(.notConnectedToInternet)]
        #expect(await !handlers.reconcile([loyalty]))

        #expect(handlers.handler(of: "game.paseo") != nil)
        #expect(workers.references.released.isEmpty)
    }

    /// The request is what keeps the worker alive, so a product whose last card
    /// has gone must give it back rather than hold a web view for a Pocket that
    /// no longer draws it.
    @Test
    func givesTheWorkerBackWhenTheLastOfAProductsCardsGoes() async throws {
        let workers = workersRunning("game.paseo")
        let handlers = makeHandlers(workers: workers)
        _ = await handlers.reconcile([loyalty])

        _ = await handlers.reconcile([])

        #expect(handlers.handler(of: "game.paseo") == nil)
        #expect(workers.references.released == ["game.paseo"])
    }

    /// A card already drawn must not be asked for twice: a second request for
    /// the same product is one the core counts and nothing gives back.
    @Test
    func doesNotAskAgainForAProductItIsAlreadyRunning() async throws {
        let workers = workersRunning("game.paseo")
        let handlers = makeHandlers(workers: workers)
        _ = await handlers.reconcile([loyalty])

        _ = await handlers.reconcile([loyalty, streak])

        #expect(workers.references.acquired == ["game.paseo"])
    }

    /// Nothing else holds a way back to the handlers, so a session that ended
    /// has to be what gives their requests back.
    @Test
    func givesEveryRequestBackWhenTheSessionStops() async throws {
        let workers = workersRunning("game.paseo")
        let handlers = makeHandlers(workers: workers)
        _ = await handlers.reconcile([loyalty])

        handlers.stop()

        #expect(workers.references.released == ["game.paseo"])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false
)

private let streak = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "streak")),
    title: "Streak",
    privileged: false
)

private func workersRunning(_ productId: ProductId) -> StubWorkerManager {
    let workers = StubWorkerManager()
    workers.publish([productId: MockProductExecution()])
    return workers
}

private func makeHandlers(
    workers: StubWorkerManager,
    published: StubPublishedCards = StubPublishedCards()
) -> PocketHandlers {
    PocketHandlers(workers: workers, published: published)
}

/// Looks every card up successfully unless a failure is queued for it.
final class StubPublishedCards: PublishedPocketCardsResolving, @unchecked Sendable {
    /// Errors thrown by successive lookups, consumed in order; once empty the
    /// lookup succeeds.
    var failures: [any Error] = []
    private(set) var lookups = 0

    func find(productId: ProductId, cardId: PocketCardId) async throws -> PublishedPocketCard {
        lookups += 1
        if !failures.isEmpty { throw failures.removeFirst() }

        return PublishedPocketCard(
            productId: productId,
            productName: productId,
            workerContentId: productId,
            definition: PocketCardDefinition(id: cardId, title: "Card", preview: .archive(path: "face.json"))
        )
    }
}
