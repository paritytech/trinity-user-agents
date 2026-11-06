import Foundation
import os
import AsyncExtensions
import IssueMonitoring
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

    /// The kept face and the first live one are each timed from the card asking for its face.
    @Test
    func reportsTheKeptFaceAndTheFirstLiveFace() async throws {
        let recorder = RecordingHostEventRecorder()
        let clock = SteppingClock(start: 100, step: 24)
        let source = RealPocketFaceSource(
            store: { KeptFaceStore() },
            streams: RunningHandler(StubFaceStreams(faces: ["one", "two"].map { RendererNode.string(text: $0) })),
            telemetry: HostLoadTelemetry(recorder: recorder, monotonicMilliseconds: { clock.read() })
        )

        for await _ in source.faces(for: loyalty) {}

        #expect(recorder.events.map(\.name) == ["host.pocket.face_loaded", "host.pocket.face_loaded"])
        #expect(recorder.events.map { $0.tags["outcome"] } == ["cache_hit", "ok"])
        #expect(recorder.events.map { $0.tags["product.id"] } == ["game.paseo", "game.paseo"])
        #expect(recorder.events.map { $0.extras["duration_ms"] } == [.integer(24), .integer(48)])
        #expect(recorder.events.allSatisfy { $0.extras["host.session_id"] == nil })
    }

    @Test
    func aStreamThatFailsBeforeItsFirstFaceReportsAnError() async throws {
        let recorder = RecordingHostEventRecorder()
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: RunningHandler(FailingFaceStreams()),
            telemetry: HostLoadTelemetry(recorder: recorder)
        )

        for await _ in source.faces(for: loyalty) {}

        #expect(recorder.events.map { $0.tags["outcome"] } == ["error"])
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

private struct FailingFaceStreams: PocketFaceStreaming {
    struct Unreachable: Error {}

    func renderFaces(for _: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        AsyncThrowingStream { $0.finish(throwing: Unreachable()) }
    }

    func send(action _: String, payload _: Data, for _: PocketCardKey) {}
}

private actor KeptFaceStore: PocketCardStore {
    func cards() async throws -> [PocketCardEntry] { [] }

    nonisolated func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        AsyncStream<[PocketCardEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func removeCard(_: PocketCardKey) async throws {}

    func add(_: PocketCardEntry, face _: RendererNode) async {}

    func face(for _: PocketCardKey) async -> RendererNode? { .string(text: "kept") }
}

private actor NoFaceStore: PocketCardStore {
    func cards() async throws -> [PocketCardEntry] { [] }

    nonisolated func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        AsyncStream<[PocketCardEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func removeCard(_: PocketCardKey) async throws {}

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
