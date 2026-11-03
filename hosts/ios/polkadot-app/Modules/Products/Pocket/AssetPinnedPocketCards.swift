import Foundation
import KeyDerivation
import Products
import TrUAPIHost

/// Humanity, backed by the governance-reserved personhood product and drawn
/// from a face bundled with the app until that product streams its own.
///
/// The reserved product id is a function of the network's dotNS suffix, so the
/// card belongs to whichever network the app is on.
struct AssetPinnedPocketCards: PinnedPocketCards {
    private struct Definition {
        let cardId: String
        let title: String
        let backingProduct: (String) throws -> ProductId

        func card(tld: String) throws -> PocketCardEntry {
            try PocketCardEntry(
                key: PocketCardKey(productId: backingProduct(tld), cardId: PocketCardId(value: cardId)),
                title: title,
                privileged: true
            )
        }
    }

    private static let definitions = [
        Definition(cardId: "humanity", title: "Humanity", backingProduct: BuiltInProduct.personhood(for:))
    ]

    private let tld: String

    init(tld: String) {
        self.tld = tld
    }

    func cards() async -> [PocketCardEntry] {
        Self.definitions.compactMap { try? $0.card(tld: tld) }
    }

    /// Answers without awaiting: a removal arrives from the core on a thread it
    /// cannot spare, and a key already names the product that must back it.
    func pinned(_ key: PocketCardKey) -> PocketCardEntry? {
        Self.definitions
            .lazy
            .compactMap { try? $0.card(tld: tld) }
            .first { $0.key == key }
    }

    /// The face shipped with the app, shown until the backing product draws its
    /// own. Bundled, so a failure here is a build defect, not product input.
    func face(for cardId: PocketCardId) async -> RendererNode? {
        guard let url = Bundle.main.url(forResource: "pocket-\(cardId.value)", withExtension: "json"),
              let json = try? String(contentsOf: url, encoding: .utf8)
        else {
            return nil
        }

        return try? parseRendererNodeJson(json: json)
    }

    /// The app's own copy of an image a pinned card's face names, so it needs
    /// no archive. Every `/` becomes `-`, so the lookup stays at the bundle root.
    ///
    /// shortcut: the app's copy shadows the product's file at the same path
    /// until the next app release.
    static func image(at path: String) -> URL? {
        guard path.hasPrefix("pocket/") else { return nil }

        return Bundle.main.url(forResource: path.replacingOccurrences(of: "/", with: "-"), withExtension: nil)
    }
}
