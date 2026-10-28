import Foundation
import AsyncExtensions
import Products
import TrUAPIHost

/// The collection the core keeps: the cards in the Pocket, pinned first, and
/// the newest face of each.
///
/// Reads, additions and removals raise what went wrong. The core tells a
/// failure apart from an empty Pocket and from a card that was already gone,
/// and a product told its removal succeeded when it did not will not ask again.
final class CorePocketCardStore: PocketCardStore, @unchecked Sendable {
    private let runtimeProvider: any TrUAPIHostRuntimeProviding
    private let pinned: any PinnedPocketCards
    private let logger: LoggerProtocol

    init(
        runtimeProvider: any TrUAPIHostRuntimeProviding,
        pinned: any PinnedPocketCards,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.runtimeProvider = runtimeProvider
        self.pinned = pinned
        self.logger = logger
    }

    func cards() async throws -> [PocketCardEntry] {
        for try await cards in try runtimeProvider.sharedRuntime().pocketCards() {
            return cards.map(Self.entry)
        }
        throw CancellationError()
    }

    /// The runtime is built on first use and can fail to build, so this waits
    /// and asks again rather than ending, which would leave the Pocket empty for
    /// the rest of the session.
    func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        AsyncStream { continuation in
            let task = Task { [runtimeProvider, logger] in
                var attempt = 0
                while !Task.isCancelled {
                    do {
                        for try await cards in try runtimeProvider.sharedRuntime().pocketCards() {
                            continuation.yield(cards.map(Self.entry))
                            attempt = 0
                        }
                    } catch {
                        logger.warning("[pocket] the collection could not be followed, asking again: \(error)")
                    }

                    try? await Task.sleep(for: pocketReopenDelay(after: attempt))
                    attempt += 1
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    func removeCard(_ key: PocketCardKey) async throws {
        _ = try await runtimeProvider.sharedRuntime().removePocketCard(
            productId: key.productId,
            cardId: key.cardId.value
        )
    }

    func add(_ card: PocketCardEntry, face: RendererNode) async throws {
        try await runtimeProvider.sharedRuntime().addPocketCard(
            productId: card.key.productId,
            cardId: card.key.cardId.value,
            title: card.title,
            face: face
        )
    }

    /// A host-placed card falls back to the face shipped with the app, so it
    /// draws before its product has drawn one.
    func face(for key: PocketCardKey) async -> RendererNode? {
        do {
            if let kept = try await runtimeProvider.sharedRuntime().pocketCardFace(
                productId: key.productId,
                cardId: key.cardId.value
            ) {
                return kept
            }
        } catch {
            logger.warning("[pocket] the kept face for '\(key.storageId)' could not be read: \(error)")
        }
        guard pinned.pinned(key) != nil else { return nil }

        return await pinned.face(for: key.cardId)
    }

    private static func entry(_ card: PocketEntry) -> PocketCardEntry {
        PocketCardEntry(
            key: PocketCardKey(productId: card.productId, cardId: PocketCardId(value: card.cardId)),
            title: card.title,
            privileged: card.privileged
        )
    }
}
