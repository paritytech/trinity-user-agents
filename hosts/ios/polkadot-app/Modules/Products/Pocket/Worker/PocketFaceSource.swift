import Foundation
import Products
import TrUAPIHost

/// Which handler is drawing a product's cards.
protocol PocketFaceStreamsResolving: Sendable {
    /// The handler running right now, or nil if none is.
    func streams(of productId: ProductId) -> (any PocketFaceStreaming)?

    /// The same, waited for. A card is on screen before the collection has been
    /// reconciled and its worker booted, and answering none then would leave it
    /// on the face it was last drawn with.
    func awaitStreams(of productId: ProductId) async -> (any PocketFaceStreaming)?
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

                // Waited for rather than asked once: the card is drawn before
                // the collection has been reconciled, and the only thing that
                // would start it again is its own identity changing.
                guard let streams = await streams.awaitStreams(of: key.productId) else {
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
        streams.streams(of: key.productId)?.send(action: action, payload: payload, for: key)
    }
}
