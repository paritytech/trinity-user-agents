import Foundation
import os
import Products
import TrUAPIHost

/// Raised while the bridge holds no snapshot of the collection, because no read
/// of it has landed yet.
struct PocketCollectionUnreadable: Error {}

/// Serves one product's slice of the collection to the core.
///
/// Both callbacks run inline on the core's dispatcher thread, so neither may
/// await: the list is answered from a snapshot, and a removal is decided and
/// answered against that same snapshot, then stored behind the answer. Until a
/// read fills that snapshot, both raise rather than answer, because the core
/// tells a failure apart from an empty Pocket and a product does not ask again
/// about a card it was told is gone.
final class ProductPocketHostBridge: PocketHostBridge, @unchecked Sendable {
    private let productId: String
    private let collection: any PocketCollection
    private let logger: LoggerProtocol
    private let snapshot = OSAllocatedUnfairLock(initialState: [PocketCard]?.none)
    private let republish = OSAllocatedUnfairLock(initialState: (([PocketCard]) -> Void)?.none)
    private let following = OSAllocatedUnfairLock(initialState: Task<Void, Never>?.none)
    private let stopped = OSAllocatedUnfairLock(initialState: false)

    init(productId: String, collection: any PocketCollection, logger: LoggerProtocol = Logger.shared) {
        self.productId = productId
        self.collection = collection
        self.logger = logger
    }

    /// Starts republishing this product's cards to the core, beginning with the
    /// snapshot as it stands. `publish` is handed the execution, which its
    /// owner closes, so it is dropped on ``stop()``.
    ///
    /// The opening publish is what closes the boot window: the snapshot is
    /// filled before the worker's script comes up, so every change taken into
    /// it until here was seen by nobody. With no snapshot there is nothing to
    /// open with, and the first read that lands publishes instead.
    func start(publish: @escaping ([PocketCard]) -> Void) {
        republish.withLock { $0 = publish }
        guard let held = snapshot.withLock({ $0 }) else { return }

        publish(held)
    }

    func stop() {
        stopped.withLock { $0 = true }
        republish.withLock { $0 = nil }
        following.withLock { held in
            held?.cancel()
            held = nil
        }
    }

    /// Takes the collection as it stands, then keeps following it.
    ///
    /// Returns once the first read has landed, because the worker's script
    /// subscribes to the card list as soon as it comes up: a snapshot filled
    /// after that point leaves the product reading an empty Pocket for the
    /// whole life of its worker.
    func begin() async {
        do {
            let held = try await collection.cards()
            take(held)
        } catch {
            logger.error("[pocket] \(productId)'s slice could not be read: \(error)")
        }

        let task = Task { [weak self, collection, logger, productId] in
            do {
                for try await cards in collection.observeCards() {
                    guard let self else { return }
                    take(cards)
                }
            } catch {
                logger.error("[pocket] \(productId) stopped following the collection: \(error)")
            }
        }

        // A stop can land while the read above is in flight. Installing the
        // follow task after that would leave one observing the collection for a
        // bridge nothing holds any more.
        guard !stopped.withLock({ $0 }) else {
            task.cancel()
            return
        }

        following.withLock { $0 = task }
    }

    /// Tells the core only if this product's own slice changed. A face
    /// streaming at frame rate changes the stored collection continuously
    /// without changing any card the core knows about.
    private func take(_ held: [PocketCardEntry]) {
        let current = held
            .filter { $0.key.productId == productId }
            .map { PocketCard(cardId: $0.key.cardId.value, privileged: $0.privileged) }

        let changed = snapshot.withLock { snapshot -> Bool in
            guard snapshot != current else { return false }
            snapshot = current
            return true
        }
        guard changed else { return }

        republish.withLock { $0 }?(current)
    }

    func listCards() throws -> [PocketCard] {
        guard let held = snapshot.withLock({ $0 }) else { throw PocketCollectionUnreadable() }

        return held
    }

    /// Answered from the snapshot the core was served in the first place, so
    /// the core's own thread waits on no storage.
    func removeCard(cardId: String) throws -> NativePocketRemoval {
        let held = try listCards()

        if held.contains(where: { $0.cardId == cardId && $0.privileged }) { return .privileged }
        guard held.contains(where: { $0.cardId == cardId }) else { return .absent }

        snapshot.withLock { $0?.removeAll { $0.cardId == cardId } }
        store(PocketCardKey(productId: productId, cardId: PocketCardId(value: cardId)))

        return .removed
    }

    /// Stores what the core was already told. A write that does not land is put
    /// right by reading the collection again, which brings the card back into
    /// the slice and republishes it, so the product and the Pocket do not end
    /// up disagreeing about what the Pocket holds.
    private func store(_ removal: PocketCardKey) {
        Task { [collection, logger, productId] in
            do {
                _ = try await collection.removeCard(removal)
            } catch {
                logger.error("[pocket] \(productId)'s removal of \(removal.cardId.value) did not store: \(error)")
                await reread()
            }
        }
    }

    private func reread() async {
        do {
            try await take(collection.cards())
        } catch {
            logger.error("[pocket] \(productId)'s slice could not be read back: \(error)")
        }
    }
}
