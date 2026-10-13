import Foundation
import AsyncExtensions
import Products
import TrUAPIHost

/// Identifies one card: the product that backs it, and the label that product
/// declared for it.
struct PocketCardKey: Hashable {
    let productId: ProductId
    let cardId: PocketCardId

    /// Flat form the card is stored under, and the one CoreData keys rows by.
    var storageId: String { "\(productId)/\(cardId.value)" }

    /// Where the card's product opens: its own page, told which card the user
    /// came from. Built the same way on every host, so a product is handed the
    /// same address whichever one it is running on.
    var launchUrl: URL? {
        var components = URLComponents()
        components.scheme = "https"
        components.host = productId
        components.queryItems = [URLQueryItem(name: "card", value: cardId.value)]

        return components.url
    }
}

/// One card in the host's Pocket collection. A privileged card is host-placed
/// and removable by nobody.
struct PocketCardEntry: Equatable {
    let key: PocketCardKey
    let title: String
    let privileged: Bool
}

/// What a removal did. A privileged card is refused with
/// ``PocketRemoveError/privileged`` instead; removing a card that is not held
/// is a success, which the core asks a host to tell apart from one it performed.
enum PocketRemoval {
    case removed
    case absent
}

enum PocketRemoveError: Error, Equatable {
    case privileged
}

/// The host-owned collection as a product sees it. Cards enter only through the
/// host's own approval flow, so there is no add here.
protocol PocketCollection: Sendable {
    /// Throws when the collection cannot be read, which a caller must tell
    /// apart from an empty Pocket.
    func cards() async throws -> [PocketCardEntry]

    /// The collection as it stands, and again after every change to it,
    /// whoever made the change. Read from storage rather than announced, so a
    /// writer cannot forget to say it wrote.
    func observeCards() -> AnyAsyncSequence<[PocketCardEntry]>

    func removeCard(_ key: PocketCardKey) async throws -> PocketRemoval
}

/// The collection as the host's own flows see it: what a product may read, plus
/// the writes only the host makes.
protocol PocketCardStore: PocketCollection {
    /// Adds a card the user approved, together with the face they approved it by.
    ///
    /// Throws when the card could not be stored, which the approval sheet must
    /// tell apart from a card the Pocket now holds.
    func add(_ card: PocketCardEntry, face: RendererNode) async throws

    /// The newest face held for `key`: the last one its product drew, or the
    /// bundled one for a host-placed card that has never drawn.
    func face(for key: PocketCardKey) async -> RendererNode?

    /// Keeps the newest face a product drew, so the card has it offline and at
    /// cold start.
    func cacheFace(_ face: RendererNode, for key: PocketCardKey) async
}

/// The cards the host itself places: present on first run, removable by nobody.
protocol PinnedPocketCards: Sendable {
    func cards() async -> [PocketCardEntry]

    /// The host-placed card `key` names, if it names one. Answers without
    /// awaiting: a removal arrives from the core on a thread it cannot spare.
    func pinned(_ key: PocketCardKey) -> PocketCardEntry?

    /// The face shipped with the app for a host-placed card.
    func face(for cardId: PocketCardId) async -> RendererNode?
}
