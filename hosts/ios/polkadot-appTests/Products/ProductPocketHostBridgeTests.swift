import Foundation
import os
import AsyncExtensions
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// Serves one product's slice of the collection to the core. Both callbacks are
/// answered synchronously on the core's own dispatcher thread.
struct ProductPocketHostBridgeTests {
    @Test
    func listsOnlyTheCallingProductsCards() async throws {
        let bridge = try await makeBridge(stored: [loyalty, otherProductCard])

        #expect(try bridge.listCards().map(\.cardId) == ["loyalty"])
    }

    @Test
    func reportsWhetherTheHostPinnedACard() async throws {
        let bridge = try await makeBridge(productId: humanity.key.productId, pinned: [humanity])

        #expect(try bridge.listCards().first?.privileged == true)
    }

    /// A privileged card is refused without touching storage, and the refusal
    /// is a distinct outcome rather than a thrown error.
    @Test
    func refusesToRemoveAPinnedCard() async throws {
        let bridge = try await makeBridge(productId: humanity.key.productId, pinned: [humanity])

        #expect(try bridge.removeCard(cardId: "humanity") == NativePocketRemoval.privileged)
    }

    @Test
    func removesACardTheProductOwns() async throws {
        let collection = InMemoryPocketCardStore()
        try await collection.add(loyalty, face: .nil)
        let bridge = try await makeBridge(collection: collection)

        #expect(try bridge.removeCard(cardId: "loyalty") == NativePocketRemoval.removed)
        #expect(try bridge.listCards().isEmpty)
    }

    /// Storage all but always takes a removal, so the core is answered before
    /// the write lands. One that does not is put right: the card comes back
    /// into the slice and the core is told, so the product and the Pocket do
    /// not end up disagreeing about what the Pocket holds.
    @Test
    func putsBackACardWhoseRemovalDidNotStore() async throws {
        let collection = InMemoryPocketCardStore()
        try await collection.add(loyalty, face: .nil)
        let bridge = try await makeBridge(collection: collection)
        let published = Published()
        bridge.start { published.record($0) }
        await collection.failWrites()

        #expect(try bridge.removeCard(cardId: "loyalty") == NativePocketRemoval.removed)
        await published.wait(forPublishes: 2)

        #expect(published.counts == [1, 1])
        #expect(try bridge.listCards().map(\.cardId) == ["loyalty"])
    }

    /// Removing a card that is not held is a success the core tells apart from
    /// one the host performed.
    @Test
    func removingAnAbsentCardIsAbsentNotAnError() async throws {
        let bridge = try await makeBridge()

        #expect(try bridge.removeCard(cardId: "loyalty") == NativePocketRemoval.absent)
    }

    /// A product cannot reach another product's card through its own bridge,
    /// even by naming it exactly.
    @Test
    func cannotRemoveAnotherProductsCard() async throws {
        let collection = InMemoryPocketCardStore()
        try await collection.add(otherProductCard, face: .nil)
        let bridge = try await makeBridge(collection: collection)

        #expect(try bridge.removeCard(cardId: "trophy") == NativePocketRemoval.absent)
        #expect(try await collection.cards().count == 1)
    }

    /// The core is told only when this product's own slice changes. A face
    /// streaming at frame rate changes the stored collection continuously
    /// without changing any card the core knows about, and another product's
    /// card changes nothing here at all.
    @Test
    func republishesOnlyWhenItsOwnSliceChanges() async throws {
        let deliveries = AsyncStream<[PocketCardEntry]>.makeStream()
        let bridge = ProductPocketHostBridge(
            productId: "game.paseo",
            collection: DrivenCollection(deliveries: deliveries.stream)
        )
        await bridge.begin()

        let published = Published()
        bridge.start { published.record($0) }

        deliveries.continuation.yield([otherProductCard])
        deliveries.continuation.yield([otherProductCard, loyalty])
        await published.wait(forPublishes: 2)

        // The leading publish is the opening one, of the snapshot as it stood.
        // Both changes are delivered in order, so a republish for the other
        // product's card would sit between the two as a repeat of the count
        // before it.
        #expect(published.counts == [0, 1])
    }

    /// A collection that could not be read is not an empty Pocket. Answering
    /// from a snapshot nothing ever filled tells a product it publishes no
    /// cards, which is the one answer it will not ask about again.
    @Test
    func raisesRatherThanListingACollectionItCouldNotRead() async throws {
        let bridge = await makeUnreadableBridge()

        #expect(throws: PocketCollectionUnreadable.self) {
            try bridge.listCards()
        }
    }

    /// Without a snapshot every card looks absent, and a product told its card
    /// is gone while the Pocket still draws it will not ask again.
    @Test
    func raisesRatherThanCallingACardAbsentFromACollectionItCouldNotRead() async throws {
        let bridge = await makeUnreadableBridge()

        #expect(throws: PocketCollectionUnreadable.self) {
            try bridge.removeCard(cardId: "loyalty")
        }
    }

    /// The opening publish is what the core's list starts from, so a read that
    /// did not land must publish nothing rather than an empty Pocket.
    @Test
    func publishesNothingUntilItHasReadTheCollection() async throws {
        let bridge = await makeUnreadableBridge()
        let published = Published()

        bridge.start { published.record($0) }

        #expect(published.counts.isEmpty)
    }

    /// The snapshot is filled before the worker's script comes up, so every
    /// change taken into it until the core can be told reached nobody. Waiting
    /// for the next change instead would wait forever: the slice has already
    /// moved, so no later refresh finds anything to report.
    @Test
    func republishesTheSnapshotItAlreadyHoldsWhenItStarts() async throws {
        let collection = InMemoryPocketCardStore()
        try await collection.add(loyalty, face: .nil)
        let bridge = try await makeBridge(collection: collection)

        let published = Published()
        bridge.start { published.record($0) }

        #expect(published.counts == [1])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false
)

private let otherProductCard = PocketCardEntry(
    key: PocketCardKey(productId: "shop.paseo", cardId: PocketCardId(value: "trophy")),
    title: "Trophy",
    privileged: false
)

private let humanity = PocketCardEntry(
    key: PocketCardKey(productId: "peopl.paseo", cardId: PocketCardId(value: "humanity")),
    title: "Humanity",
    privileged: true
)

private func makeBridge(
    productId: String = "game.paseo",
    pinned: [PocketCardEntry] = [],
    stored: [PocketCardEntry] = [],
    collection: InMemoryPocketCardStore? = nil
) async throws -> ProductPocketHostBridge {
    let held = collection ?? InMemoryPocketCardStore(pinned: InMemoryPinnedCards(pinned))
    for card in stored {
        try await held.add(card, face: .nil)
    }
    let bridge = ProductPocketHostBridge(productId: productId, collection: held)
    await bridge.begin()
    return bridge
}

private func makeUnreadableBridge() async -> ProductPocketHostBridge {
    let bridge = ProductPocketHostBridge(productId: "game.paseo", collection: UnreadableCollection())
    await bridge.begin()
    return bridge
}

/// Storage the app cannot read: the first read fails and nothing follows it,
/// which is what leaves the bridge with no snapshot to answer from.
private struct UnreadableCollection: PocketCollection {
    struct Unavailable: Error {}

    func cards() async throws -> [PocketCardEntry] { throw Unavailable() }

    func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        AsyncStream<[PocketCardEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func removeCard(_: PocketCardKey) async throws -> PocketRemoval { throw Unavailable() }
}

/// A collection the test hands each change to, so they are observed in the
/// order they were made and an assertion runs on a state that has arrived
/// rather than after a delay.
private struct DrivenCollection: PocketCollection {
    let deliveries: AsyncStream<[PocketCardEntry]>

    func cards() async throws -> [PocketCardEntry] { [] }

    func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        deliveries.eraseToAnyAsyncSequence()
    }

    func removeCard(_: PocketCardKey) async throws -> PocketRemoval { .absent }
}

/// Records what the bridge asked to republish, and lets a test wait for a
/// publish to land rather than for a delay to pass.
private final class Published: @unchecked Sendable {
    private let held = OSAllocatedUnfairLock(initialState: [Int]())
    private let arrivals = AsyncStream<Int>.makeStream()

    var counts: [Int] { held.withLock { $0 } }

    func record(_ cards: [PocketCard]) {
        let arrived = held.withLock { held -> Int in
            held.append(cards.count)
            return held.count
        }
        arrivals.continuation.yield(arrived)
    }

    /// Returns once `count` publishes have landed.
    func wait(forPublishes count: Int) async {
        guard counts.count < count else { return }

        for await arrived in arrivals.stream where arrived >= count {
            return
        }
    }
}
