import Foundation
import AsyncExtensions
import Products
import TrUAPIHost
@testable import polkadot_app

/// The collection held in memory, standing in for the CoreData one.
///
/// Snapshots are seeded with what is held and sent on every write, the way a
/// CoreData snapshot subscription behaves, so a test can assert that a surface
/// followed a write nobody announced.
actor InMemoryPocketCardStore: PocketCardStore {
    struct Unavailable: Error {}

    private var stored: [PocketCardEntry]
    private var faces: [PocketCardKey: RendererNode] = [:]
    private let pinned: any PinnedPocketCards
    private var readsFail = false
    private var writesFail = false
    private let snapshots: AsyncCurrentValueSubject<[PocketCardEntry]>

    init(_ stored: [PocketCardEntry] = [], pinned: any PinnedPocketCards = InMemoryPinnedCards([])) {
        self.stored = stored
        self.pinned = pinned
        snapshots = AsyncCurrentValueSubject(stored)
    }

    /// Storage the app can no longer read, which is a failure rather than an
    /// empty Pocket.
    func failReads() {
        readsFail = true
    }

    /// Storage the app can no longer write to: a card that was not added, and
    /// a removal that did not happen rather than one that found nothing.
    func failWrites() {
        writesFail = true
    }

    func cards() async throws -> [PocketCardEntry] {
        if readsFail { throw Unavailable() }

        return await settle(stored)
    }

    nonisolated func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        let pinned = pinned

        return snapshots
            .map { [pinned] added in
                let placed = await pinned.cards()
                let placedKeys = Set(placed.map(\.key))

                return placed + added.filter { !placedKeys.contains($0.key) }
            }
            .eraseToAnyAsyncSequence()
    }

    func removeCard(_ key: PocketCardKey) async throws -> PocketRemoval {
        guard pinned.pinned(key) == nil else { throw PocketRemoveError.privileged }
        if writesFail { throw Unavailable() }

        let before = stored.count
        stored.removeAll { $0.key == key }
        faces[key] = nil
        snapshots.send(stored)

        return stored.count != before ? .removed : .absent
    }

    func add(_ card: PocketCardEntry, face: RendererNode) async throws {
        if writesFail { throw Unavailable() }

        stored.removeAll { $0.key == card.key }
        stored.append(card)
        faces[card.key] = face
        snapshots.send(stored)
    }

    func face(for key: PocketCardKey) async -> RendererNode? {
        if let kept = faces[key] { return kept }
        guard pinned.pinned(key) != nil else { return nil }

        return await pinned.face(for: key.cardId)
    }

    func cacheFace(_ face: RendererNode, for key: PocketCardKey) async {
        faces[key] = face
    }

    private func settle(_ added: [PocketCardEntry]) async -> [PocketCardEntry] {
        let placed = await pinned.cards()
        let placedKeys = Set(placed.map(\.key))

        return placed + added.filter { !placedKeys.contains($0.key) }
    }
}

/// The cards the host itself places, as a test supplies them.
struct InMemoryPinnedCards: PinnedPocketCards {
    private let cardsHeld: [PocketCardEntry]
    private let bundledFace: RendererNode?

    init(_ cardsHeld: [PocketCardEntry], face: RendererNode? = nil) {
        self.cardsHeld = cardsHeld
        bundledFace = face
    }

    func cards() async -> [PocketCardEntry] { cardsHeld }

    func pinned(_ key: PocketCardKey) -> PocketCardEntry? {
        cardsHeld.first { $0.key == key }
    }

    func face(for _: PocketCardId) async -> RendererNode? { bundledFace }
}
