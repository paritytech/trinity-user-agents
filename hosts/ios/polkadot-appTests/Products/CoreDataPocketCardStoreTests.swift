import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The collection as it is actually stored, against the same CoreData model the
/// app runs on. Every other Pocket suite runs on the in-memory stand-in, so the
/// rows, the mappers and the order they come back in are only proved here.
@Suite("CoreDataPocketCardStore")
struct CoreDataPocketCardStoreTests {
    // MARK: - Ordering

    /// The Pocket keeps the order cards were added in rather than whatever
    /// order the rows happen to come back in.
    @Test
    func listsAddedCardsOldestFirst() async throws {
        let store = makeStore()

        try await store.add(loyalty, face: .nil)
        try await store.add(streak, face: .nil)

        #expect(try await store.cards().map(\.key) == [loyalty.key, streak.key])
    }

    /// Host-placed cards keep the front of the collection, so a product cannot
    /// push Humanity down the Pocket by adding its own.
    @Test
    func listsPinnedCardsBeforeAddedOnes() async throws {
        let store = makeStore(pinned: [humanity])

        try await store.add(loyalty, face: .nil)

        #expect(try await store.cards().map(\.key) == [humanity.key, loyalty.key])
    }

    /// A card the user added before the host came to pin it is listed once, as
    /// the pinned one, so it cannot be removed by the copy that is not pinned.
    @Test
    func listsACardHeldBothWaysOnlyOnce() async throws {
        let store = makeStore(pinned: [humanity])
        try await store.add(PocketCardEntry(key: humanity.key, title: "stale copy", privileged: false), face: .nil)
        try await store.add(loyalty, face: .nil)

        let cards = try await store.cards()

        #expect(cards.map(\.key) == [humanity.key, loyalty.key])
        #expect(cards.first == humanity)
    }

    // MARK: - Removal

    @Test
    func refusesToRemoveAPrivilegedCard() async {
        let store = makeStore(pinned: [humanity])

        await #expect(throws: PocketRemoveError.privileged) {
            try await store.removeCard(humanity.key)
        }
    }

    /// The core asks a host to tell a removal it performed apart from one it
    /// had nothing to do, and both are successes.
    @Test
    func removingACardThatIsNotHeldSucceedsAsAbsent() async throws {
        let store = makeStore()

        #expect(try await store.removeCard(loyalty.key) == .absent)
    }

    @Test
    func removingAHeldCardDropsIt() async throws {
        let store = makeStore()
        try await store.add(loyalty, face: .nil)

        #expect(try await store.removeCard(loyalty.key) == .removed)
        #expect(try await store.cards().isEmpty)
    }

    /// A card removed while another is held must take only its own row.
    @Test
    func removesOnlyTheCardItWasAsked() async throws {
        let store = makeStore()
        try await store.add(loyalty, face: .nil)
        try await store.add(streak, face: .nil)

        _ = try await store.removeCard(loyalty.key)

        #expect(try await store.cards().map(\.key) == [streak.key])
    }

    // MARK: - Faces

    /// The approval sheet showed a face, and that is the face the Pocket draws:
    /// the card is stored with it rather than re-read from anywhere.
    @Test
    func keepsTheFaceACardWasAddedWith() async throws {
        let store = makeStore()

        try await store.add(loyalty, face: .string(text: "approved"))

        #expect(await store.face(for: loyalty.key) == .string(text: "approved"))
    }

    /// A card added again must be drawn by the face the user approved the
    /// second time, not the one left behind by the first.
    @Test
    func dropsTheFaceWhenTheCardIsRemoved() async throws {
        let store = makeStore()
        try await store.add(loyalty, face: .string(text: "approved"))

        _ = try await store.removeCard(loyalty.key)

        #expect(await store.face(for: loyalty.key) == nil)
    }

    /// A host-placed card draws on first run, before its product has ever run,
    /// from the face shipped with the app.
    @Test
    func fallsBackToTheBundledFaceForAPinnedCard() async throws {
        let store = makeStore(pinned: [humanity], bundledFace: .string(text: "bundled"))

        #expect(await store.face(for: humanity.key) == .string(text: "bundled"))
    }

    /// Once its product draws, a pinned card keeps that face rather than the
    /// bundled one, so it is right at the next cold start too.
    @Test
    func prefersTheKeptFaceOverTheBundledOne() async throws {
        let store = makeStore(pinned: [humanity], bundledFace: .string(text: "bundled"))

        await store.cacheFace(.string(text: "drawn"), for: humanity.key)

        #expect(await store.face(for: humanity.key) == .string(text: "drawn"))
    }

    /// A host-placed card is placed on every run rather than stored, so the row
    /// that keeps the face its product drew must not read back as a card the
    /// user added: listed that way it would also be offered for removal.
    @Test
    func keepingAFaceDoesNotAddTheCardToTheCollection() async throws {
        let store = makeStore()

        await store.cacheFace(.string(text: "drawn"), for: loyalty.key)

        #expect(try await store.cards().isEmpty)
    }

    /// What the user added and the newest face share a row, so writing the face
    /// must not take the membership with it.
    @Test
    func keepingAFaceLeavesTheCardTheUserAdded() async throws {
        let store = makeStore()
        try await store.add(loyalty, face: .string(text: "approved"))

        await store.cacheFace(.string(text: "drawn"), for: loyalty.key)

        #expect(try await store.cards().map(\.key) == [loyalty.key])
        #expect(await store.face(for: loyalty.key) == .string(text: "drawn"))
    }

    /// A card nobody holds has no face to answer with, pinned or not.
    @Test
    func answersNoFaceForACardItDoesNotHold() async {
        let store = makeStore(bundledFace: .string(text: "bundled"))

        #expect(await store.face(for: loyalty.key) == nil)
    }

    // MARK: - Observation

    /// The Wallet tab and each product's bridge follow this rather than being
    /// told, so a write nobody announced still has to reach them.
    @Test
    func sendsTheCollectionAgainAfterAWrite() async throws {
        let store = makeStore()
        let seen = Seen()

        let following = Task {
            for try await cards in store.observeCards() {
                await seen.record(cards)
            }
        }
        defer { following.cancel() }

        try await store.add(loyalty, face: .nil)
        try await waitUntil { await seen.last?.contains { $0.key == loyalty.key } == true }

        #expect(await seen.last?.map(\.key) == [loyalty.key])
    }

    /// A card and its face share a row, so keeping a face writes the very row
    /// the collection is followed through. A product animating its card writes
    /// one every couple of seconds, and sending the collection again each time
    /// would re-run every surface that follows it.
    @Test
    func doesNotSendTheCollectionAgainWhenOnlyAFaceChanged() async throws {
        let store = makeStore()
        try await store.add(loyalty, face: .nil)
        let seen = Seen()

        let following = Task {
            for try await cards in store.observeCards() {
                await seen.record(cards)
            }
        }
        defer { following.cancel() }
        try await waitUntil { await seen.all.count == 1 }

        await store.cacheFace(.string(text: "drawn"), for: loyalty.key)
        try await store.add(streak, face: .nil)
        try await waitUntil { await seen.last?.count == 2 }

        #expect(await seen.all.map(\.count) == [1, 2])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false
)

private let streak = PocketCardEntry(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "streak")),
    title: "Streak",
    privileged: false
)

private let humanity = PocketCardEntry(
    key: PocketCardKey(productId: "peopl.paseo", cardId: PocketCardId(value: "humanity")),
    title: "Humanity",
    privileged: true
)

private func makeStore(
    pinned: [PocketCardEntry] = [],
    bundledFace: RendererNode? = nil
) -> CoreDataPocketCardStore {
    CoreDataPocketCardStore(
        pinned: InMemoryPinnedCards(pinned, face: bundledFace),
        storageFacade: UserDataStorageTestFacade()
    )
}

/// CoreData delivers its snapshots on its own queue, so the assertions wait on
/// what arrived rather than on the clock.
private actor Seen {
    private(set) var all: [[PocketCardEntry]] = []

    var last: [PocketCardEntry]? { all.last }

    func record(_ cards: [PocketCardEntry]) {
        all.append(cards)
    }
}

private func waitUntil(_ condition: () async -> Bool) async throws {
    for _ in 0 ..< 50 {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(50))
    }
}
