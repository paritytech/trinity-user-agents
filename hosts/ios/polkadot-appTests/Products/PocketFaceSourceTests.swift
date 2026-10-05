import Foundation
import AsyncExtensions
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product animating its card draws at frame rate. Keeping every face would
/// serialise the whole tree into storage once per frame, on the path the screen
/// is waiting on; keeping none would leave the card blank at cold start.
struct PocketFaceSourceTests {
    @Test
    func drawsEveryFaceButKeepsOnlyTheNewest() async throws {
        let store = RecordingFaceStore()
        let source = RealPocketFaceSource(
            store: { store },
            streams: { _ in
                StubFaceStreams(faces: ["one", "two", "three"].map { RendererNode.string(text: $0) })
            }
        )

        var drawn: [String] = []
        for await face in source.faces(for: loyalty) {
            drawn.append(face.text ?? "")
        }

        #expect(drawn == ["one", "two", "three"])
        // The first face is worth keeping at once; the two that follow it
        // arrive inside one interval, so only the face the card settled on is
        // written down after them.
        #expect(await store.kept.compactMap(\.text) == ["one", "three"])
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

private actor RecordingFaceStore: PocketCardStore {
    private(set) var kept: [RendererNode] = []

    func cards() async throws -> [PocketCardEntry] { [] }

    nonisolated func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        AsyncStream<[PocketCardEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func removeCard(_: PocketCardKey) async throws -> PocketRemoval { .absent }

    func add(_: PocketCardEntry, face _: RendererNode) async {}

    func face(for _: PocketCardKey) async -> RendererNode? { nil }

    func cacheFace(_ face: RendererNode, for _: PocketCardKey) async {
        kept.append(face)
    }
}
