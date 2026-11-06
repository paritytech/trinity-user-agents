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

    @Test
    func aStreamThatFailsAfterItsFirstFaceReportsOnlyTheFace() async throws {
        let recorder = RecordingHostEventRecorder()
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: RunningHandler(FaceThenFailure()),
            telemetry: HostLoadTelemetry(recorder: recorder)
        )

        for await _ in source.faces(for: loyalty) {}

        #expect(recorder.events.map { $0.tags["outcome"] } == ["ok"])
    }

    /// A stream torn down with the card is the card going, not the product failing.
    @Test
    func aCancelledStreamIsAbandonedNotFailed() async throws {
        let recorder = RecordingHostEventRecorder()
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: RunningHandler(CancelledFaceStreams()),
            telemetry: HostLoadTelemetry(recorder: recorder)
        )

        for await _ in source.faces(for: loyalty) {}

        #expect(recorder.events.map { $0.tags["outcome"] } == ["abandoned"])
    }

    /// The face is joined to the worker that draws it, and says whether that worker was already up.
    @Test
    func aFaceCarriesItsWorkersSessionAndTemperature() async throws {
        let recorder = RecordingHostEventRecorder()
        let session = UUID()
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: RunningHandler(
                StubFaceStreams(faces: [.string(text: "live")]),
                status: PocketWorkerStatus(hostSessionId: session, isRunning: true)
            ),
            telemetry: HostLoadTelemetry(recorder: recorder)
        )

        for await _ in source.faces(for: loyalty) {}

        #expect(recorder.events.first?.tags["worker"] == "warm")
        #expect(recorder.events.first?.tags["host.session_id"] == session.uuidString.lowercased())
    }

    @Test
    func aFaceNotDrawnInItsWindowTimesOutAndTheCardKeepsWaiting() async throws {
        let recorder = RecordingHostEventRecorder()
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: RunningHandler(SilentFaceStreams()),
            telemetry: HostLoadTelemetry(recorder: recorder),
            faceTimeout: .milliseconds(10)
        )

        let drawing = Task { for await _ in source.faces(for: loyalty) {} }
        try await recorder.waitForEvents(count: 1)
        drawing.cancel()
        await drawing.value

        #expect(recorder.events.map { $0.tags["outcome"] } == ["timeout"])
        #expect(recorder.events.first?.tags["worker"] == "cold")
    }

    @Test
    func aCardWhoseProductHasNoWorkerReportsNoWorker() async throws {
        let recorder = RecordingHostEventRecorder()
        let handlers = UnservedProduct()
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: handlers,
            telemetry: HostLoadTelemetry(recorder: recorder)
        )

        let drawing = Task { for await _ in source.faces(for: loyalty) {} }
        handlers.refuse(loyalty.productId)
        try await recorder.waitForEvents(count: 1)
        drawing.cancel()
        await drawing.value

        #expect(recorder.events.map { $0.tags["outcome"] } == ["no_worker"])
    }

    @Test
    func aCardThatGoesBeforeItsFaceIsAbandoned() async throws {
        let recorder = RecordingHostEventRecorder()
        let source = RealPocketFaceSource(
            store: { NoFaceStore() },
            streams: RunningHandler(SilentFaceStreams()),
            telemetry: HostLoadTelemetry(recorder: recorder)
        )

        let drawing = Task { for await _ in source.faces(for: loyalty) {} }
        try await Task.sleep(for: .milliseconds(20))
        drawing.cancel()
        await drawing.value
        try await recorder.waitForEvents(count: 1)

        #expect(recorder.events.map { $0.tags["outcome"] } == ["abandoned"])
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
    let status: PocketWorkerStatus

    init(_ handler: any PocketFaceStreaming, status: PocketWorkerStatus = .none) {
        self.handler = handler
        self.status = status
    }

    func streams(of _: ProductId) -> (any PocketFaceStreaming)? { handler }

    func awaitStreams(of _: ProductId) async -> (any PocketFaceStreaming)? { handler }

    func workerStatus(of _: ProductId) -> PocketWorkerStatus { status }

    func unservedProducts() -> AnyAsyncSequence<Set<ProductId>> {
        AsyncStream<Set<ProductId>> { _ in }.eraseToAnyAsyncSequence()
    }
}

/// A product no handler ever starts for, until the test says it has none.
private final class UnservedProduct: PocketFaceStreamsResolving, @unchecked Sendable {
    private let unserved = AsyncCurrentValueSubject<Set<ProductId>>([])

    func refuse(_ productId: ProductId) {
        unserved.send([productId])
    }

    func streams(of _: ProductId) -> (any PocketFaceStreaming)? { nil }

    func awaitStreams(of _: ProductId) async -> (any PocketFaceStreaming)? {
        // Never answered: the real handlers keep a card waiting for a later reconcile.
        try? await Task.sleep(for: .seconds(3_600))
        return nil
    }

    func workerStatus(of _: ProductId) -> PocketWorkerStatus { .none }

    func unservedProducts() -> AnyAsyncSequence<Set<ProductId>> {
        unserved.eraseToAnyAsyncSequence()
    }
}

private extension PocketWorkerStatus {
    static let none = PocketWorkerStatus(hostSessionId: nil, isRunning: false)
}

private struct FaceThenFailure: PocketFaceStreaming {
    struct Dropped: Error {}

    func renderFaces(for _: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        AsyncThrowingStream { continuation in
            continuation.yield(.string(text: "live"))
            continuation.finish(throwing: Dropped())
        }
    }

    func send(action _: String, payload _: Data, for _: PocketCardKey) {}
}

/// Draws nothing until cancelled.
private struct SilentFaceStreams: PocketFaceStreaming {
    func renderFaces(for _: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        AsyncThrowingStream { continuation in
            let held = Task {
                try? await Task.sleep(for: .seconds(3_600))
                continuation.finish()
            }
            continuation.onTermination = { _ in held.cancel() }
        }
    }

    func send(action _: String, payload _: Data, for _: PocketCardKey) {}
}

private struct CancelledFaceStreams: PocketFaceStreaming {
    func renderFaces(for _: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        AsyncThrowingStream { $0.finish(throwing: CancellationError()) }
    }

    func send(action _: String, payload _: Data, for _: PocketCardKey) {}
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

    func workerStatus(of _: ProductId) -> PocketWorkerStatus { .none }

    func unservedProducts() -> AnyAsyncSequence<Set<ProductId>> {
        AsyncStream<Set<ProductId>> { _ in }.eraseToAnyAsyncSequence()
    }
}
