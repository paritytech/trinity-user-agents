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
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: RunningHandler(
                StubFaceStreams(faces: ["one", "two", "three"].map { RendererNode.string(text: $0) })
            )
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
        let source = RealPocketFaceSource(store: { NoFaceStore() }, streams: handlers)

        let drawing = Task { () -> [String] in
            var drawn: [String] = []
            for await face in source.faces(for: loyalty) {
                drawn.append(face.text ?? "")
            }
            return drawn
        }
        handlers.arrive(StubFaceStreams(faces: [.string(text: "live")]))

        #expect(await drawing.value == ["live"])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

private extension RendererNode {
    var text: String? {
        guard case let .string(text) = self else { return nil }

        return text
    }
}

private struct StubFaceStreams: PocketFaceStreaming {
    let faces: [RendererNode]

    func renderFaces(for _: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        AsyncThrowingStream { continuation in
            faces.forEach { continuation.yield($0) }
            continuation.finish()
        }
    }

    func send(action _: String, payload _: Data, for _: PocketCardKey) {}
}

private actor NoFaceStore: PocketCardStore {
    func cards() async throws -> [PocketCardEntry] { [] }

    nonisolated func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        AsyncStream<[PocketCardEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func removeCard(_: PocketCardKey) async throws -> PocketRemoval { .absent }

    func add(_: PocketCardEntry, face _: RendererNode) async {}

    func face(for _: PocketCardKey) async -> RendererNode? { nil }
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
