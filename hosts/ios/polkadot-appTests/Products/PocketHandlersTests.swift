import Foundation
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

private let gameProductId = try! ProductId("game.paseo")

@MainActor
@Suite(.timeLimit(.minutes(1)))
struct PocketHandlersTests {
    @Test func onlyThePresentedCardTakesAClaimSharedByItsViews() async throws {
        let workers = StubWorkerManager()
        let published = StubPublishedCards()
        published.checkLookup = { cardId in #expect(cardId == loyalty.key.cardId) }
        let handlers = makeHandlers(workers: workers, published: published)
        let dismissWallet = handlers.present(loyalty.key, expanded: false)
        handlers.settle([loyalty, streak])
        _ = try #require(await handlers.awaitStreams(for: loyalty.key))
        let dismissOpened = handlers.present(loyalty.key, expanded: true)
        dismissWallet()
        dismissWallet()
        #expect(workers.references.log == ["acquire \(gameProductId)"])
        dismissOpened()
        #expect(workers.references.log == ["acquire \(gameProductId)", "release \(gameProductId)"])
    }

    @Test func chatAndPocketOwnTheirWorkAcrossARenderRestart() async throws {
        let workers = StubWorkerManager()
        let execution = MockProductExecution()
        execution.keepsRenderStreamOpen = true
        workers.publish([gameProductId: execution])
        let handlers = makeHandlers(workers: workers)
        handlers.settle([loyalty])
        let leavePocket = handlers.present(loyalty.key, expanded: false)
        defer { leavePocket() }
        let streams = try #require(await handlers.awaitStreams(for: loyalty.key))
        let chat = TrUAPIChatHandler(productId: gameProductId, workers: workers)
        try await chat.start(messagingSupport: .init(bot: nil, context: nil))
        let drawing = Task { for try await _ in streams.renderFaces(for: loyalty.key) {} }
        defer { drawing.cancel() }

        do {
            try await withTimeout(.hangGuard) { await execution.awaitRenderRequest() }
            let first = try #require(execution.renderContinuations.first)
            first.finish(throwing: URLError(.networkConnectionLost))
            try await withTimeout(.hangGuard) { await execution.awaitRenderRequest() }
            #expect(workers.references.acquired == [gameProductId, gameProductId])
            #expect(workers.references.released.isEmpty)
            leavePocket()
            #expect(workers.references.released == [gameProductId])
        } catch {
            await chat.dispose()
            throw error
        }
        await chat.dispose()
        #expect(workers.references.released == workers.references.acquired)
    }

    @Test func expandedCardWithABackHoldsBothSidesUntilCollapse() async throws {
        let published = StubPublishedCards()
        published.back = "back.json"
        let workers = StubWorkerManager()
        let handlers = makeHandlers(workers: workers, published: published)
        handlers.settle([loyalty])
        let dismissOpened = handlers.present(loyalty.key, expanded: true)
        _ = try #require(await handlers.awaitStreams(for: loyalty.key))
        let dismissWallet = handlers.present(loyalty.key, expanded: false)
        #expect(workers.references.acquired == [gameProductId, gameProductId])
        dismissOpened()
        #expect(workers.references.released == [gameProductId])
        dismissWallet()
        #expect(workers.references.released == [gameProductId, gameProductId])
    }

    @Test func removingOneCardReleasesOnlyItsClaimAndShutdownReturnsTheRest() async throws {
        let workers = StubWorkerManager()
        let handlers = makeHandlers(workers: workers)
        handlers.settle([loyalty, streak])
        let dismissFirst = handlers.present(loyalty.key, expanded: false)
        let dismissSecond = handlers.present(streak.key, expanded: false)
        _ = try #require(await handlers.awaitStreams(for: loyalty.key))
        _ = try #require(await handlers.awaitStreams(for: streak.key))
        handlers.settle([streak])
        #expect(workers.references.released == [gameProductId])
        handlers.stop()
        dismissFirst()
        dismissSecond()
        #expect(workers.references.released == workers.references.acquired)
    }

    @Test func anUnpublishedVisibleCardKeepsItsCachedFaceWithoutTakingAClaim() async {
        let workers = StubWorkerManager()
        let published = StubPublishedCards()
        published.failures = [PocketPublishError.noPocket]
        let handlers = makeHandlers(workers: workers, published: published)
        handlers.settle([loyalty])
        let dismiss = handlers.present(loyalty.key, expanded: false)
        #expect(await handlers.awaitStreams(for: loyalty.key) == nil)
        dismiss()
        #expect(workers.references.log.isEmpty)
    }

    @Test func aTransientRefreshKeepsBothSidesUntilTheOfferIsWithdrawn() async throws {
        let releases = AsyncStream.makeStream(of: ProductId.self)
        let references = StubWorkerReferences(onRelease: { releases.continuation.yield($0) })
        let workers = StubWorkerManager(references: references)
        let published = StubPublishedCards()
        published.back = "back.json"
        let handlers = makeHandlers(workers: workers, published: published)
        handlers.settle([loyalty])
        let dismiss = handlers.present(loyalty.key, expanded: true)
        _ = try #require(await handlers.awaitStreams(for: loyalty.key))
        await confirmation("transient refresh is retried", expectedCount: 2) { attempt in
            published.checkLookup = { _ in
                attempt()
                #expect(references.acquired == [gameProductId, gameProductId])
                #expect(references.released.isEmpty)
            }
            published.failures = [URLError(.notConnectedToInternet), PocketPublishError.unknownCard]
            handlers.settle([loyalty])
            var released = releases.stream.makeAsyncIterator()
            #expect(await released.next() == gameProductId)
            #expect(await released.next() == gameProductId)
        }
        #expect(await handlers.awaitStreams(for: loyalty.key) == nil)
        dismiss()
        #expect(references.released == references.acquired)
    }

    @Test func aTransientManifestFailureIsRetriedWhileTheCardIsVisible() async throws {
        let workers = StubWorkerManager()
        let published = StubPublishedCards()
        published.failures = [URLError(.notConnectedToInternet)]
        let handlers = makeHandlers(workers: workers, published: published)
        handlers.settle([loyalty])
        let dismiss = handlers.present(loyalty.key, expanded: false)
        _ = try #require(await handlers.awaitStreams(for: loyalty.key))
        dismiss()
        #expect(workers.references.acquired == [gameProductId])
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
    var back: String?
    var checkLookup: @Sendable (PocketCardId) -> Void = { _ in }

    func find(productId: ProductId, cardId: PocketCardId) async throws -> PublishedPocketCard {
        checkLookup(cardId)
        if !failures.isEmpty { throw failures.removeFirst() }

        return PublishedPocketCard(
            productId: productId,
            productName: productId.rawValue,
            workerContentId: productId,
            definition: PocketCardDefinition(
                id: cardId,
                title: "Card",
                preview: .archive(path: "face.json"),
                back: back
            )
        )
    }
}
