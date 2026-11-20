import Foundation
import Products
import StructuredConcurrency
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

    /// The back, read like the front. Nil once the card's definition says it
    /// has no back.
    func backFaces(for key: PocketCardKey) -> AsyncStream<RendererNode?>

    func send(action: String, payload: Data, for key: PocketCardKey, side: PocketCardSide)
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
    private let published: any PublishedPocketCardsResolving
    private let previews: PocketPreviewLoader
    private let logger: LoggerProtocol

    init(
        store: @escaping @Sendable () async -> (any PocketCardStore)?,
        streams: any PocketFaceStreamsResolving,
        published: any PublishedPocketCardsResolving,
        previews: PocketPreviewLoader,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.streams = streams
        self.published = published
        self.previews = previews
        self.logger = logger
    }

    func faces(for key: PocketCardKey) -> AsyncStream<RendererNode> {
        AsyncStream { continuation in
            let task = Task {
                if let kept = await store()?.face(for: key, side: .front) {
                    continuation.yield(kept)
                }
                await drawLive(key, side: .front) { continuation.yield($0) }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// No back render is asked for before the manifest says the card has a
    /// back, so a card that does not turn never wakes its worker for one.
    func backFaces(for key: PocketCardKey) -> AsyncStream<RendererNode?> {
        AsyncStream { continuation in
            let task = Task {
                let kept = await store()?.face(for: key, side: .back)
                if let kept { continuation.yield(kept) }

                guard let card = await answeredLookup(of: key) else {
                    continuation.finish()
                    return
                }
                guard let shipped = card.definition.back else {
                    continuation.yield(nil)
                    continuation.finish()
                    return
                }
                if kept == nil {
                    do {
                        try await continuation.yield(previews.load(contentId: card.workerContentId, preview: shipped))
                    } catch {
                        logger.warning("[pocket] no shipped back for \(key.cardId.value): \(error)")
                    }
                }

                await drawLive(key, side: .back) { continuation.yield($0) }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func send(action: String, payload: Data, for key: PocketCardKey, side: PocketCardSide) {
        streams.streams(of: key.productId)?.send(action: action, payload: payload, for: key, side: side)
    }
}

private extension RealPocketFaceSource {
    func drawLive(_ key: PocketCardKey, side: PocketCardSide, _ draw: (RendererNode) -> Void) async {
        // Waited for rather than asked once: the card is drawn before the
        // collection has been reconciled, and the only thing that would start
        // it again is its own identity changing.
        guard let streams = await streams.awaitStreams(of: key.productId) else { return }

        do {
            for try await face in streams.renderFaces(for: key, side: side) {
                draw(face)
            }
        } catch {
            logger.error("[pocket] the \(side) stream for \(key.cardId.value) ended: \(error)")
        }
    }

    /// The card as its manifest publishes it, asked again until the manifest
    /// answers: a read that did not land says nothing about the card. Nil when
    /// the answer is that there is no such card to draw.
    func answeredLookup(of key: PocketCardKey) async -> PublishedPocketCard? {
        do {
            return try await withRetry(strategy: CardLookupRetry(key: key, logger: logger)) { [published] in
                try await published.find(productId: key.productId, cardId: key.cardId)
            }
        } catch let refusal as PocketPublishError {
            logger.debug("[pocket] \(key.cardId.value) has no published back: \(refusal)")
            return nil
        } catch {
            return nil
        }
    }
}

/// Asks again after any failure but a refusal, which is the manifest's answer.
private struct CardLookupRetry: RetryStrategy {
    let key: PocketCardKey
    let logger: LoggerProtocol
    var failures = 0

    mutating func decideOnFailure(_ error: Error) async throws {
        if error is PocketPublishError { throw error }

        logger.warning("[pocket] \(key.cardId.value) could not be looked up, asking again: \(error)")
        try await Task.sleep(for: pocketReopenDelay(after: failures))
        failures += 1
    }
}
