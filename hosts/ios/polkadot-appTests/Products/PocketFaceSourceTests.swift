import Foundation
import os
import AsyncExtensions
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct PocketFaceSourceTests {
    @Test
    func drawsEveryFaceTheProductDraws() async throws {
        let source = makeSource(
            streams: StubFaceStreams(front: ["one", "two", "three"].map { RendererNode.string(text: $0) })
        )

        var drawn: [String] = []
        for await face in source.faces(for: loyalty) {
            drawn.append(face.text ?? "")
        }

        #expect(drawn == ["one", "two", "three"])
    }

    /// The wallet tab draws its cards before the collection has been reconciled
    /// and a worker booted. Ending the stream there would leave the card on the
    /// face it was last drawn with and nothing would ever start it again: the
    /// card only re-runs its task when its own identity changes.
    @Test
    func waitsForAHandlerThatIsNotRunningYet() async throws {
        let handlers = LateHandlers()
        let source = makeSource(streams: handlers)

        let drawing = Task { () -> [String] in
            var drawn: [String] = []
            for await face in source.faces(for: loyalty) {
                drawn.append(face.text ?? "")
            }
            return drawn
        }
        handlers.arrive(StubFaceStreams(front: [.string(text: "live")]))

        #expect(await drawing.value == ["live"])
    }

    // MARK: - The back

    /// The back shows something at once: the one the core kept from the last
    /// live draw, else the one the product ships, and then every live back the
    /// worker draws.
    @Test(arguments: [(nil, ["shipped", "live"]), ("kept", ["kept", "live"])])
    func showsTheKeptOrShippedBackThenTheLiveOne(kept: String?, drawn: [String]) async {
        let streams = StubFaceStreams(back: [.string(text: "live")])
        let source = makeSource(keptBack: kept.map { .string(text: $0) }, streams: streams)

        #expect(await source.backTexts(for: loyalty) == drawn)
        #expect(streams.renderedSides == [.back])
    }

    /// A back render asked for before the manifest has said the card has a
    /// back would wake a worker for a card that does not turn. The kept back
    /// still shows, so a card opened offline turns to what it last drew.
    @Test
    func showsTheKeptBackAndRendersNothingWhileTheLookupIsUnanswered() async throws {
        let streams = StubFaceStreams(back: [.string(text: "live")])
        let source = makeSource(
            keptBack: .string(text: "kept"),
            catalog: FindStub { try await Task.sleep(for: .seconds(3_600)); throw CancellationError() },
            streams: streams
        )

        var backs = source.backFaces(for: loyalty).makeAsyncIterator()
        let first = await backs.next()

        #expect(first??.text == "kept")
        #expect(streams.renderedSides.isEmpty)
    }

    /// Only the manifest's own answer takes a back away. A card it defines
    /// without one has none, even one kept from before; a manifest that cannot
    /// be read says nothing about the card, so a kept back stays.
    @Test(arguments: [
        (FindStub.definition(back: nil), ["kept", "none"]),
        (FindStub { throw PocketPublishError.unreadableProduct }, ["kept"])
    ])
    func dropsTheBackOnlyWhenTheManifestDefinesNone(catalog: FindStub, drawn: [String]) async {
        let streams = StubFaceStreams(back: [.string(text: "live")])
        let source = makeSource(keptBack: .string(text: "kept"), catalog: catalog, streams: streams)

        #expect(await source.backTexts(for: loyalty) == drawn)
        #expect(streams.renderedSides.isEmpty)
    }

    /// The two sides are separate renders, so a back the worker refuses to draw
    /// must not cost the card its front.
    @Test
    func keepsTheFrontStreamingWhenTheBackRenderFails() async {
        let streams = StubFaceStreams(front: [.string(text: "front")], failsBack: true)
        let source = makeSource(streams: streams)

        let backs = await source.backTexts(for: loyalty)
        var fronts: [String] = []
        for await face in source.faces(for: loyalty) {
            fronts.append(face.text ?? "")
        }

        #expect(backs == ["shipped"])
        #expect(fronts == ["front"])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: try! ProductId("game.paseo"), cardId: PocketCardId(value: "loyalty"))

private extension RendererNode {
    var text: String? {
        guard case let .string(text) = self else { return nil }

        return text
    }
}

private func makeSource(
    keptBack: RendererNode? = nil,
    catalog: any PublishedPocketCardsResolving = FindStub.definition(back: .archive(path: "faces/loyalty-back.json")),
    streams: any PocketFaceStreaming
) -> RealPocketFaceSource {
    makeSource(keptBack: keptBack, catalog: catalog, streams: RunningHandler(streams))
}

private func makeSource(
    keptBack: RendererNode? = nil,
    catalog: any PublishedPocketCardsResolving = FindStub.definition(back: .archive(path: "faces/loyalty-back.json")),
    streams: any PocketFaceStreamsResolving
) -> RealPocketFaceSource {
    RealPocketFaceSource(
        store: { KeptFaces(back: keptBack) },
        streams: streams,
        published: catalog,
        previews: PocketPreviewLoader(
            archive: StubArchive(contents: Data(#"{"tag":"String","value":{"text":"shipped"}}"#.utf8)),
            fetch: refusingFetch
        )
    )
}

private extension RealPocketFaceSource {
    /// Every back drawn until the stream ends, with "none" for no back.
    func backTexts(for key: PocketCardKey) async -> [String] {
        var drawn: [String] = []
        for await back in backFaces(for: key) {
            drawn.append(back.map { $0.text ?? "" } ?? "none")
        }
        return drawn
    }
}

/// Answers every lookup with what `find` gives.
struct FindStub: PublishedPocketCardsResolving {
    let find: @Sendable () async throws -> PublishedPocketCard

    init(_ find: @escaping @Sendable () async throws -> PublishedPocketCard) {
        self.find = find
    }

    static func definition(back: PocketCardPreview?) -> FindStub {
        FindStub {
            PublishedPocketCard(
                productId: loyalty.productId,
                productName: "Game",
                workerContentId: loyalty.productId,
                definition: PocketCardDefinition(
                    id: loyalty.cardId,
                    title: "Loyalty",
                    preview: .archive(path: "faces/loyalty.json"),
                    back: back
                )
            )
        }
    }

    func find(productId _: ProductId, cardId _: PocketCardId) async throws -> PublishedPocketCard {
        try await find()
    }
}

private final class StubFaceStreams: PocketFaceStreaming, @unchecked Sendable {
    private let front: [RendererNode]
    private let back: [RendererNode]
    private let failsBack: Bool
    private let rendered = OSAllocatedUnfairLock(initialState: [PocketCardSide]())

    init(front: [RendererNode] = [], back: [RendererNode] = [], failsBack: Bool = false) {
        self.front = front
        self.back = back
        self.failsBack = failsBack
    }

    var renderedSides: [PocketCardSide] { rendered.withLock { $0 } }

    func renderFaces(for _: PocketCardKey, side: PocketCardSide) -> AsyncThrowingStream<RendererNode, Error> {
        rendered.withLock { $0.append(side) }
        let faces = side == .front ? front : back
        let fails = side == .back && failsBack

        return AsyncThrowingStream { continuation in
            faces.forEach { continuation.yield($0) }
            continuation.finish(throwing: fails ? RenderRefused() : nil)
        }
    }

    func send(action _: String, payload _: Data, for _: PocketCardKey, side _: PocketCardSide) {}
}

private actor KeptFaces: PocketCardStore {
    let back: RendererNode?

    init(back: RendererNode?) {
        self.back = back
    }

    func cards() async throws -> [PocketCardEntry] { [] }

    nonisolated func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        AsyncStream<[PocketCardEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func removeCard(_: PocketCardKey) async throws {}

    func add(_: PocketCardEntry, face _: RendererNode) async {}

    func face(for _: PocketCardKey, side: PocketCardSide) async -> RendererNode? {
        side == .back ? back : nil
    }
}

/// A product whose handler is already drawing its cards.
private struct RunningHandler: PocketFaceStreamsResolving {
    let handler: any PocketFaceStreaming

    init(_ handler: any PocketFaceStreaming) {
        self.handler = handler
    }

    func streams(of _: ProductId) -> (any PocketFaceStreaming)? { handler }

    func awaitStreams(of _: ProductId) async -> (any PocketFaceStreaming)? { handler }
}

/// A product with no handler running: it only starts after the card has
/// already begun drawing, which is what a cold start looks like.
private final class LateHandlers: PocketFaceStreamsResolving, @unchecked Sendable {
    private let arrivals = AsyncStream<any PocketFaceStreaming>.makeStream()

    func arrive(_ streams: any PocketFaceStreaming) {
        arrivals.continuation.yield(streams)
    }

    func streams(of _: ProductId) -> (any PocketFaceStreaming)? { nil }

    func awaitStreams(of _: ProductId) async -> (any PocketFaceStreaming)? {
        for await streams in arrivals.stream {
            return streams
        }

        return nil
    }
}

private struct RenderRefused: Error {}
