import Foundation
import os
import AsyncExtensions
import Products

/// The Pocket handlers running right now: one per product holding a card its
/// worker offers.
///
/// Separate from ``ProductPocketService`` because what starts and stops a
/// handler is decided by the collection and the product manifests alone, and by
/// nothing else the session holds.
final class PocketHandlers: PocketFaceStreamsResolving, @unchecked Sendable {
    private let workers: any TrUAPIWorkerManaging
    private let published: any PublishedPocketCardsResolving
    private let logger: LoggerProtocol

    private let running = OSAllocatedUnfairLock(initialState: Running())
    /// Sent on every change so a card already on screen can wait for the
    /// handler that will draw it, rather than being told there is none.
    private let announced = AsyncCurrentValueSubject<[ProductId: TrUAPIPocketHandler]>([:])

    private struct Running {
        var handlers: [ProductId: TrUAPIPocketHandler] = [:]
        var settling: Task<Void, Never>?
        var stopped = false
    }

    init(
        workers: any TrUAPIWorkerManaging,
        published: any PublishedPocketCardsResolving,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.workers = workers
        self.published = published
        self.logger = logger
    }

    func handler(of productId: ProductId) -> TrUAPIPocketHandler? {
        running.withLock { $0.handlers[productId] }
    }

    func streams(of productId: ProductId) -> (any PocketFaceStreaming)? {
        handler(of: productId)
    }

    func awaitStreams(of productId: ProductId) async -> (any PocketFaceStreaming)? {
        for await handler in announced.map({ $0[productId] }) {
            if let handler { return handler }
        }

        return nil
    }

    /// Reconciles beside the collection rather than in front of the next
    /// change, and keeps asking while a lookup is unanswered: a chain read that
    /// did not land must not leave a card static until the collection next
    /// moves, which may be never.
    func settle(_ cards: [PocketCardEntry]) {
        let task = Task { [weak self] in
            var attempt = 0
            while !Task.isCancelled {
                guard let self, await !reconcile(cards) else { return }

                try? await Task.sleep(for: Self.retryDelay(after: attempt))
                attempt += 1
            }
        }

        let stopped = running.withLock { running -> Bool in
            guard !running.stopped else { return true }

            running.settling?.cancel()
            running.settling = task
            return false
        }
        if stopped { task.cancel() }
    }

    /// Gives back every worker request these handlers took. Nothing else holds
    /// a way back to them.
    func stop() {
        let held = running.withLock { running -> [TrUAPIPocketHandler] in
            running.stopped = true
            running.settling?.cancel()
            running.settling = nil

            let held = Array(running.handlers.values)
            running.handlers = [:]
            return held
        }

        announced.send([:])

        for handler in held {
            handler.dispose()
        }
    }

    /// Starts a handler for every product holding a card its worker offers, and
    /// stops the ones left without. The offer check is what keeps the
    /// host-placed card every Pocket carries from booting a worker for a
    /// product that draws nothing.
    ///
    /// Answers whether every card was looked up conclusively.
    func reconcile(_ cards: [PocketCardEntry]) async -> Bool {
        var offered: Set<ProductId> = []
        var unread: Set<ProductId> = []

        for card in cards where !offered.contains(card.key.productId) {
            switch await offer(of: card.key) {
            case .offered: offered.insert(card.key.productId)
            case .notOffered: break
            case .unanswered: unread.insert(card.key.productId)
            }
        }

        // The lock runs its closure as concurrently executing code, which may
        // read only values that can no longer change.
        let wanted = offered
        let unanswered = unread

        let (started, stopped) = running.withLock { running -> ([ProductId], [TrUAPIPocketHandler]) in
            guard !running.stopped else { return ([], []) }

            // A read that did not land is not the product saying its card is
            // gone, so a handler already drawing one keeps running rather than
            // being stopped and booted again by the retry.
            let stopped = running.handlers.filter {
                !wanted.contains($0.key) && !unanswered.contains($0.key)
            }
            for productId in stopped.keys {
                running.handlers[productId] = nil
            }

            let started = wanted.filter { running.handlers[$0] == nil }
            for productId in started {
                running.handlers[productId] = TrUAPIPocketHandler(
                    productId: productId,
                    workers: workers,
                    logger: logger
                )
            }

            announced.send(running.handlers)
            return (Array(started), Array(stopped.values))
        }

        for handler in stopped {
            handler.dispose()
        }

        for productId in started {
            await start(productId)
        }

        return unanswered.isEmpty
    }
}

private extension PocketHandlers {
    /// Doubling from a second up to half a minute, so a chain that is simply
    /// slow is picked up at once and one that is down is not asked on a loop.
    static func retryDelay(after attempt: Int) -> Duration {
        .seconds(min(1 << min(attempt, 5), 30))
    }

    func start(_ productId: ProductId) async {
        guard let handler = handler(of: productId) else { return }

        do {
            try await handler.start()
        } catch {
            logger.error("[pocket] \(productId)'s cards have no worker to draw them: \(error)")

            // Dropped rather than left in place, so the next change is free to
            // ask for the worker again.
            let dropped = running.withLock { running -> TrUAPIPocketHandler? in
                let dropped = running.handlers.removeValue(forKey: productId)
                announced.send(running.handlers)
                return dropped
            }
            dropped?.dispose()
        }
    }

    enum Offer {
        case offered
        case notOffered
        /// A read that did not land, which is not the product saying no.
        case unanswered
    }

    func offer(of key: PocketCardKey) async -> Offer {
        do {
            _ = try await published.find(productId: key.productId, cardId: key.cardId)
            return .offered
        } catch let refusal as PocketPublishError {
            logger.debug("[pocket] \(key.cardId.value): \(refusal); it keeps the face it has")
            return .notOffered
        } catch {
            logger.warning("[pocket] \(key.cardId.value) could not be looked up, asking again: \(error)")
            return .unanswered
        }
    }
}
