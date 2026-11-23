import Foundation
import Products
import TrUAPIHost

/// Routes drawing and actions only for a live, published card.
protocol PocketFaceStreamsResolving: Sendable {
    /// The renderer for this visible card, when available.
    func streams(for key: PocketCardKey) -> (any PocketFaceStreaming)?

    /// Wait for the card's eligibility check or the end of the session.
    func awaitStreams(for key: PocketCardKey) async -> (any PocketFaceStreaming)?
}

/// Where a card's face comes from: what the host already holds, then everything
/// the product draws.
protocol PocketFaceSourcing: Sendable {
    func faces(for key: PocketCardKey) -> AsyncStream<RendererNode>

    func send(action: String, payload: Data, for key: PocketCardKey)
}

/// Shows the kept face at once, then every live face the product draws. The
/// core keeps the newest.
///
/// A failure ends the stream quietly rather than throwing. The card keeps the
/// face it has and waits for its product to draw again.
struct RealPocketFaceSource: PocketFaceSourcing {
    /// Resolved per call rather than held: the collection is not readable until
    /// the network's dotNS suffix is, and the cards are drawn before that.
    private let store: @Sendable () async -> (any PocketCardStore)?
    private let streams: any PocketFaceStreamsResolving
    private let logger: LoggerProtocol

    init(
        store: @escaping @Sendable () async -> (any PocketCardStore)?,
        streams: any PocketFaceStreamsResolving,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.streams = streams
        self.logger = logger
    }

    func faces(for key: PocketCardKey) -> AsyncStream<RendererNode> {
        AsyncStream { continuation in
            let task = Task {
                if let kept = await store()?.face(for: key) {
                    continuation.yield(kept)
                }

                // The cached face is already visible while its manifest is being checked.
                guard let streams = await streams.awaitStreams(for: key) else {
                    continuation.finish()
                    return
                }

                do {
                    for try await face in streams.renderFaces(for: key) {
                        continuation.yield(face)
                    }
                } catch {
                    logger.error("[pocket] the face stream for \(key.cardId.value) ended: \(error)")
                }

                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func send(action: String, payload: Data, for key: PocketCardKey) {
        streams.streams(for: key)?.send(action: action, payload: payload, for: key)
    }
}
