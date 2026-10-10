import Foundation
import AsyncExtensions
import Operation_iOS
import Products
import TrUAPIHost

/// The settled collection: the cards the host placed, then the ones the user
/// added, each with the newest face held for it.
///
/// One type between the callers and CoreData, over one row per card. A row
/// carries what the user added, the newest face its product drew, or both: a
/// host-placed card is placed on every run rather than added, so its row holds
/// only a face.
///
/// Reads, additions and removals raise what went wrong. The core tells a
/// failure apart from an empty Pocket and from a card that was already gone,
/// and a product told its removal succeeded when it did not will not ask again.
/// Surfaces with nowhere to put a failure, the Wallet tab first of all, fall
/// back to an empty list themselves.
///
/// Faces are the exception: one that no longer reads is answered as none, and
/// the card keeps its place and waits for its product to draw again.
final class CoreDataPocketCardStore: PocketCardStore, @unchecked Sendable {
    private let pinned: any PinnedPocketCards
    private let storageFacade: StorageFacadeProtocol
    private let cardMapper = AnyCoreDataMapper(PocketCardMapper())
    private let cardRows: AnyDataProviderRepository<StoredPocketCard>
    private let faceRows: AnyDataProviderRepository<StoredPocketCardFace>
    private let logger: LoggerProtocol

    init(
        pinned: any PinnedPocketCards,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.pinned = pinned
        self.storageFacade = storageFacade
        cardRows = AnyDataProviderRepository(
            storageFacade.createRepository(mapper: AnyCoreDataMapper(PocketCardMapper()))
        )
        faceRows = AnyDataProviderRepository(
            storageFacade.createRepository(mapper: AnyCoreDataMapper(PocketCardFaceMapper()))
        )
        self.logger = logger
    }

    /// Pinned cards keep the front, and a card the user added before the host
    /// came to pin it is listed once, as the pinned one. The stored copy is
    /// not privileged, so listing it too would offer a permanent card for
    /// removal.
    func cards() async throws -> [PocketCardEntry] {
        try await settle(added())
    }

    /// Followed rather than announced: every surface showing the collection
    /// sees a write whoever made it, including the core removing a card through
    /// its own bridge.
    func observeCards() -> AnyAsyncSequence<[PocketCardEntry]> {
        let pinned = pinned

        return storageFacade
            .subscribeSnapshot(mapper: cardMapper)
            .map { rows in
                let placed = await pinned.cards()
                let placedKeys = Set(placed.map(\.key))

                return placed + Self.added(rows).filter { !placedKeys.contains($0.key) }
            }
            // A card and its face share a row, so keeping a face writes the row
            // this is followed through. Every surface that follows the
            // collection would re-read one that did not change, once per face a
            // product draws.
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }

    func removeCard(_ key: PocketCardKey) async throws -> PocketRemoval {
        guard pinned.pinned(key) == nil else { throw PocketRemoveError.privileged }

        // Looked up by id rather than over the whole collection, because this
        // runs while the core waits on the answer.
        let held = try await cardRows
            .fetchOperation(by: { key.storageId }, options: .init())
            .asyncExecute() != nil

        // The whole row goes: a card added again must not inherit the face the
        // last one was approved by.
        try await cardRows.saveOperation({ [] }, { [key.storageId] }).asyncExecute()

        return held ? .removed : .absent
    }

    func add(_ card: PocketCardEntry, face: RendererNode) async throws {
        let stored = StoredPocketCard(
            key: card.key,
            membership: .init(title: card.title, addedAt: Date()),
            face: nil
        )
        try await cardRows.saveOperation({ [stored] }, { [] }).asyncExecute()

        await cacheFace(face, for: card.key)
    }

    /// A host-placed card falls back to the face shipped with the app, so it
    /// draws on first run and again after its product stops drawing.
    func face(for key: PocketCardKey) async -> RendererNode? {
        if let kept = await keptFace(for: key) { return kept }
        guard pinned.pinned(key) != nil else { return nil }

        return await pinned.face(for: key.cardId)
    }

    func cacheFace(_ face: RendererNode, for key: PocketCardKey) async {
        do {
            let stored = StoredPocketCardFace(key: key, face: encodeRendererNode(node: face))
            try await faceRows.saveOperation({ [stored] }, { [] }).asyncExecute()
        } catch {
            logger.warning("pocket: the newest face for '\(key.storageId)' could not be kept: \(error)")
        }
    }
}

private extension CoreDataPocketCardStore {
    func added() async throws -> [PocketCardEntry] {
        try await Self.added(cardRows.fetchAllOperation(with: .init()).asyncExecute())
    }

    /// The cards the user added, oldest first. A row holding only a face is not
    /// one of them, and the Pocket keeps the order cards were added in rather
    /// than whatever order the rows come back in.
    static func added(_ rows: [StoredPocketCard]) -> [PocketCardEntry] {
        rows
            .compactMap { row in row.membership.map { (key: row.key, membership: $0) } }
            .sorted { $0.membership.addedAt < $1.membership.addedAt }
            .map { PocketCardEntry(key: $0.key, title: $0.membership.title, privileged: false) }
    }

    func settle(_ added: [PocketCardEntry]) async -> [PocketCardEntry] {
        let placed = await pinned.cards()
        let placedKeys = Set(placed.map(\.key))

        return placed + added.filter { !placedKeys.contains($0.key) }
    }

    /// A face the running app can no longer read is answered as none: the card
    /// keeps its place and waits for its product to draw again.
    func keptFace(for key: PocketCardKey) async -> RendererNode? {
        do {
            guard let face = try await cardRows
                .fetchOperation(by: { key.storageId }, options: .init())
                .asyncExecute()?
                .face
            else {
                return nil
            }

            return try decodeRendererNode(bytes: face)
        } catch {
            logger.warning("pocket: the kept face for '\(key.storageId)' no longer reads: \(error)")
            return nil
        }
    }
}
