import Foundation
import Products
import TrUAPIHost

/// The cards the core pins, each drawn from a face bundled with the app until
/// its product streams its own.
struct AssetPinnedPocketCards: PinnedPocketCards {
    /// Read on the network the key's own product names, so it answers without
    /// knowing the app's network.
    func pinned(_ key: PocketCardKey) -> PocketCardEntry? {
        guard let tld = key.productId.rawValue.split(separator: ".").last,
              let cards = try? pinnedPocketCards(networkSuffix: String(tld))
        else {
            return nil
        }

        return cards
            .first { $0.productId == key.productId && $0.cardId == key.cardId.value }
            .map { PocketCardEntry(key: key, title: $0.title, privileged: true) }
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
