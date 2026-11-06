import Foundation
import IssueMonitoring
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
    private let telemetry: HostLoadTelemetry
    private let logger: LoggerProtocol

    init(
        store: @escaping @Sendable () async -> (any PocketCardStore)?,
        streams: any PocketFaceStreamsResolving,
        telemetry: HostLoadTelemetry = .disabled,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.streams = streams
        self.telemetry = telemetry
        self.logger = logger
    }

    func faces(for key: PocketCardKey) -> AsyncStream<RendererNode> {
        let requestedAt = telemetry.now()

        return AsyncStream { continuation in
            let task = Task {
                if let kept = await store()?.face(for: key) {
                    continuation.yield(kept)
                    reportFace(of: key, outcome: .cacheHit, requestedAt: requestedAt)
                }

                // Waited for rather than asked once: the card is drawn before
                // the collection has been reconciled, and the only thing that
                // would start it again is its own identity changing.
                guard let streams = await streams.awaitStreams(of: key.productId) else {
                    continuation.finish()
                    return
                }

                var drewLiveFace = false

                do {
                    for try await face in streams.renderFaces(for: key) {
                        continuation.yield(face)

                        if !drewLiveFace {
                            drewLiveFace = true
                            reportFace(of: key, outcome: .ok, requestedAt: requestedAt)
                        }
                    }
                } catch {
                    logger.error("[pocket] the face stream for \(key.cardId.value) ended: \(error)")

                    if !drewLiveFace, !(error is CancellationError) {
                        reportFace(of: key, outcome: .error, requestedAt: requestedAt)
                    }
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

private extension RealPocketFaceSource {
    func reportFace(of key: PocketCardKey, outcome: HostEventOutcome, requestedAt: Int?) {
        guard let requestedAt else { return }

        telemetry.record { origin, now in
            .pocketFaceLoaded(
                productID: key.productId.rawValue,
                outcome: outcome,
                durationMilliseconds: now - requestedAt,
                origin: origin,
                monotonicMilliseconds: now
            )
        }
    }
}
