import Foundation
import os
import AsyncExtensions
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

    /// Telemetry only: the worker behind the product's cards.
    func workerStatus(of productId: ProductId) -> PocketWorkerStatus

    /// Telemetry only: products no worker will draw for, because none offers the
    /// card or the worker failed to start.
    func unservedProducts() -> AnyAsyncSequence<Set<ProductId>>
}

struct PocketWorkerStatus: Equatable, Sendable {
    let hostSessionId: UUID?
    let isRunning: Bool
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
    private let faceTimeout: Duration
    private let logger: LoggerProtocol

    init(
        store: @escaping @Sendable () async -> (any PocketCardStore)?,
        streams: any PocketFaceStreamsResolving,
        telemetry: HostLoadTelemetry = .disabled,
        faceTimeout: Duration = .seconds(30),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.streams = streams
        self.telemetry = telemetry
        self.faceTimeout = faceTimeout
        self.logger = logger
    }

    func faces(for key: PocketCardKey) -> AsyncStream<RendererNode> {
        let load = PocketFaceLoad(key: key, streams: streams, telemetry: telemetry, timeout: faceTimeout)

        return AsyncStream { continuation in
            let task = Task {
                if let kept = await store()?.face(for: key) {
                    continuation.yield(kept)
                    load?.reportKeptFace()
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
                            load?.finish(.ok)
                        }
                    }
                } catch {
                    logger.error("[pocket] the face stream for \(key.cardId.value) ended: \(error)")

                    if error is CancellationError {
                        load?.finish(.abandoned)
                    } else {
                        load?.finish(.error)
                    }
                }

                // A stream that ended before drawing anything did not load the face.
                if !drewLiveFace, !Task.isCancelled {
                    load?.finish(.error)
                }

                continuation.finish()
            }
            load?.watch()
            continuation.onTermination = { _ in
                task.cancel()
                load?.finish(.abandoned)
            }
        }
    }

    func send(action: String, payload: Data, for key: PocketCardKey) {
        streams.streams(of: key.productId)?.send(action: action, payload: payload, for: key)
    }
}

/// One card's wait for its first live face, reported once as `ok`, `error`, `timeout`,
/// `no_worker` or `abandoned`, whichever comes first. A kept face is reported on its own
/// beforehand. It only watches: nothing here changes what the card is drawn with.
private final class PocketFaceLoad: Sendable {
    private struct State {
        var finished = false
        var watchers: [Task<Void, Never>] = []
    }

    private let key: PocketCardKey
    private let streams: any PocketFaceStreamsResolving
    private let telemetry: HostLoadTelemetry
    private let timeout: Duration
    private let requestedAt: Int
    private let worker: HostWorkerState
    private let state = OSAllocatedUnfairLock(initialState: State())

    init?(
        key: PocketCardKey,
        streams: any PocketFaceStreamsResolving,
        telemetry: HostLoadTelemetry,
        timeout: Duration
    ) {
        guard let requestedAt = telemetry.now() else { return nil }

        self.key = key
        self.streams = streams
        self.telemetry = telemetry
        self.timeout = timeout
        self.requestedAt = requestedAt
        worker = streams.workerStatus(of: key.productId).isRunning ? .warm : .cold
    }

    func reportKeptFace() {
        report(.cacheHit)
    }

    func finish(_ outcome: HostEventOutcome) {
        let (first, watchers) = state.withLock { state -> (Bool, [Task<Void, Never>]) in
            guard !state.finished else { return (false, []) }

            state.finished = true
            defer { state.watchers = [] }
            return (true, state.watchers)
        }
        guard first else { return }

        watchers.forEach { $0.cancel() }
        report(outcome)
    }

    func watch() {
        let productId = key.productId
        let watchers = [
            Task { [timeout] in
                do {
                    try await Task.sleep(for: timeout)
                    finish(.timeout)
                } catch {}
            },
            Task { [streams] in
                do {
                    for try await unserved in streams.unservedProducts() where unserved.contains(productId) {
                        finish(.noWorker)
                        return
                    }
                } catch {}
            }
        ]

        let finished = state.withLock { state -> Bool in
            guard !state.finished else { return true }

            state.watchers = watchers
            return false
        }
        if finished { watchers.forEach { $0.cancel() } }
    }

    private func report(_ outcome: HostEventOutcome) {
        let hostSessionId = streams.workerStatus(of: key.productId).hostSessionId

        telemetry.record { origin, now in
            HostLoadEvent.pocketFaceLoaded(
                productID: key.productId.rawValue,
                outcome: outcome,
                worker: worker,
                durationMilliseconds: now - requestedAt,
                hostSessionID: hostSessionId,
                origin: origin,
                monotonicMilliseconds: now
            )
        }
    }
}
