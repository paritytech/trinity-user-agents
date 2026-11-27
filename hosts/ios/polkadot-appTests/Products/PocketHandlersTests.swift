import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

private let gameProductId = try! ProductId("game.paseo")

/// A handler is what asks for a product's worker, so what starts one decides
/// which web views the session runs. A handler too many burns one for a product
/// that draws nothing; one too few leaves its cards on the face they were last
/// drawn with.
struct PocketHandlersTests {
    @Test
    func keepsOneClaimUntilTheProductsLastCardIsRemoved() async throws {
        let workers = StubWorkerRuntime()
        let handlers = makeHandlers(workers: workers)

        #expect(await handlers.reconcile([loyalty, streak]))
        let handler = try #require(handlers.handler(of: gameProductId))
        #expect(await handlers.reconcile([streak]))
        #expect(handlers.handler(of: gameProductId) === handler)
        #expect(workers.references.acquired == [gameProductId])
        #expect(workers.references.releaseCounts == [0])

        #expect(await handlers.reconcile([]))
        #expect(handlers.handler(of: gameProductId) == nil)
        #expect(workers.references.releaseCounts == [1])
    }

    @Test
    func removingAndReaddingAPocketCardReleasesOnlyItsOwnClaim() async throws {
        let workers = StubWorkerRuntime()
        let chat = TrUAPIChatHandler(productId: gameProductId, runtime: { workers }, context: ProductWorkerContext())
        try await chat.start(messagingSupport: .init(bot: nil, context: nil))
        let handlers = makeHandlers(workers: workers)

        _ = await handlers.reconcile([loyalty])
        _ = await handlers.reconcile([])
        #expect(workers.references.releaseCounts == [0, 1])
        _ = await handlers.reconcile([loyalty])
        #expect(workers.references.releaseCounts == [0, 1, 0])

        handlers.stop()
        #expect(workers.references.releaseCounts == [0, 1, 1])
        await chat.dispose()
        #expect(workers.references.releaseCounts == [1, 1, 1])
    }

    /// A product that offers no such card draws nothing, and a handler for it
    /// would boot a worker anyway. The host-placed card every Pocket carries
    /// sits on a product like that.
    @Test
    func runsNoHandlerForACardTheProductDoesNotOffer() async throws {
        let published = StubPublishedCards()
        published.failures = [PocketPublishError.noPocket]
        let workers = StubWorkerRuntime()
        let handlers = makeHandlers(workers: workers, published: published)

        #expect(await handlers.reconcile([loyalty]))

        #expect(handlers.handler(of: gameProductId) == nil)
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
        let workers = StubWorkerRuntime()
        let handlers = makeHandlers(workers: workers, published: published)

        #expect(await !handlers.reconcile([loyalty]))
        #expect(handlers.handler(of: gameProductId) == nil)

        #expect(await handlers.reconcile([loyalty]))
        #expect(handlers.handler(of: gameProductId) != nil)
    }

    /// A read that did not land is not the product saying its card is gone.
    /// Stopping a handler on one would take down a card that is drawing, and
    /// the retry a moment later would boot the worker all over again.
    @Test
    func keepsAHandlerWhoseCardLookupDidNotLand() async throws {
        let published = StubPublishedCards()
        let workers = StubWorkerRuntime()
        let handlers = makeHandlers(workers: workers, published: published)
        _ = await handlers.reconcile([loyalty])

        published.failures = [URLError(.notConnectedToInternet)]
        #expect(await !handlers.reconcile([loyalty]))

        #expect(handlers.handler(of: gameProductId) != nil)
        #expect(workers.references.released.isEmpty)
    }

    /// Nothing else holds a way back to the handlers, so a session that ended
    /// has to be what gives their requests back.
    @Test
    func givesEveryRequestBackWhenTheSessionStops() async throws {
        let workers = StubWorkerRuntime()
        let handlers = makeHandlers(workers: workers)
        let other = try PocketCardEntry(
            key: PocketCardKey(productId: ProductId("other.paseo"), cardId: loyalty.key.cardId),
            title: "Other", privileged: false
        )
        _ = await handlers.reconcile([loyalty, other])

        handlers.stop()

        #expect(workers.references.releaseCounts == [1, 1])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardEntry(
    key: PocketCardKey(productId: gameProductId, cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false
)

private let streak = PocketCardEntry(
    key: PocketCardKey(productId: gameProductId, cardId: PocketCardId(value: "streak")),
    title: "Streak",
    privileged: false
)

private func makeHandlers(
    workers: StubWorkerRuntime,
    published: StubPublishedCards = StubPublishedCards()
) -> PocketHandlers {
    PocketHandlers(runtime: { workers }, published: published)
}

/// Looks every card up successfully unless a failure is queued for it.
private final class StubPublishedCards: PublishedPocketCardsResolving, @unchecked Sendable {
    /// Errors thrown by successive lookups, consumed in order; once empty the
    /// lookup succeeds.
    var failures: [any Error] = []

    func find(productId: ProductId, cardId: PocketCardId) async throws -> PublishedPocketCard {
        if !failures.isEmpty { throw failures.removeFirst() }

        return PublishedPocketCard(
            productId: productId,
            productName: productId.rawValue,
            workerContentId: productId,
            definition: PocketCardDefinition(id: cardId, title: "Card", preview: .archive(path: "face.json"))
        )
    }
}
