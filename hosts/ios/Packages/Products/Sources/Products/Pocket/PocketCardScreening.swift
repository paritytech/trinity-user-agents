import Foundation

/// The label a product declares for one of its cards, unique within that product.
public struct PocketCardId: Hashable, Sendable {
    public let value: String

    public init(value: String) {
        self.value = value
    }
}

/// Screens the id and the title a product declares for a card.
///
/// Both rules belong to the core, which applies them to every Pocket call and
/// to the card id inside a deeplink. A host that screens by its own rules
/// refuses cards the core accepted, so this carries the core's screening in
/// rather than restating it: `Products` cannot see `TrUAPIHost`, and the two
/// rules are not the same one. An id is addressed, so it refuses the joiners
/// and variation selectors that let two ids draw alike. A title is only drawn,
/// and emoji and Persian need exactly those characters.
public struct PocketCardScreening: Sendable {
    private let screenId: @Sendable (String) throws -> String
    private let screenTitle: @Sendable (String) throws -> String

    public init(
        id: @escaping @Sendable (String) throws -> String,
        title: @escaping @Sendable (String) throws -> String
    ) {
        screenId = id
        screenTitle = title
    }

    public func id(_ raw: String) throws -> PocketCardId {
        try PocketCardId(value: screenId(raw))
    }

    public func title(_ raw: String) throws -> String {
        try screenTitle(raw)
    }
}
