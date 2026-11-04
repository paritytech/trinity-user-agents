import struct TrUAPIHost.ProductId
import Foundation
import KeyDerivation
import Keystore_iOS
import Products

/// A product the host itself puts in chat: present on first run, with no add, install or discover
/// step, and removable by nobody. The chat analogue of Pocket's privileged card
/// (`docs/rfcs/pocket-modality.md`), shaped like its `AssetPinnedPocketCards`.
///
/// Placing one grants no permissions; it still prompts like any other product.
struct HostPlacedProduct: Sendable {
    /// Must match the room the product registers, which iOS gives the host no way to check. A
    /// product that registers another room, or none, gets that chat as well and leaves this one
    /// empty beside it. Pocket's `Definition` names its `cardId` the same way, but owns card ids
    /// in a way no host owns a room id.
    let roomId: String

    /// Shown until the published manifest resolves and supplies the real name.
    let fallbackName: String

    /// Reserved identity as a function of the network TLD, so one entry covers every chain.
    let productId: @Sendable (String) throws -> ProductId
}

enum HostPlacedProducts {
    /// Whether the host places these is ``SettingsManagerProtocol/isHostPlacementEnabled``.
    ///
    /// The Polkadot room is moving from the game's `jollity` to Humanity's `polkadot`
    /// (individuality-apps#1281). This is the first stage: both are placed, so the game's worker
    /// keeps running until Humanity's worker has taken the room over and the game's worker has
    /// cancelled the pushes only it can cancel. A later release stops placing the game.
    static let all: [HostPlacedProduct] = [
        HostPlacedProduct(roomId: "polkadot", fallbackName: "Polkadot", productId: BuiltInProduct.personhood(for:)),
        HostPlacedProduct(roomId: "jollity", fallbackName: "Jollity", productId: BuiltInProduct.dim2(for:))
    ]

    /// Whether `roomId` of `productId` is a room the host placed. Only that room: a placed
    /// product's other rooms, such as Humanity's earlier `humanity`, are its own and get no
    /// placement treatment.
    ///
    /// Read against the candidate's own TLD, so it answers without the chain read that listing
    /// needs — the trick Pocket's `pinned(key:)` uses. An exact match, not a prefix test, so
    /// `app.dim2.dot` is a different publisher and is not host-placed.
    static func placesRoom(_ roomId: String?, of productId: ProductId) -> Bool {
        let labels = productId.rawValue.split(separator: ".")
        guard let roomId, labels.count > 1, let tld = labels.last else {
            return false
        }

        return all.contains { $0.roomId == roomId && (try? $0.productId(String(tld))) == productId }
    }
}

protocol HostPlacedProductsProviding: Sendable {
    func placedProductIds() async -> [ProductId]
}

final class HostPlacedProductsProvider: HostPlacedProductsProviding, @unchecked Sendable {
    private let settingsManager: SettingsManagerProtocol
    private let tldProvider: DotNsTldProviding

    init(settingsManager: SettingsManagerProtocol, tldProvider: DotNsTldProviding) {
        self.settingsManager = settingsManager
        self.tldProvider = tldProvider
    }

    func placedProductIds() async -> [ProductId] {
        guard settingsManager.isHostPlacementEnabled,
              let tld = try? await tldProvider.currentTldElseResolve() else {
            return []
        }

        return HostPlacedProducts.all.compactMap { try? $0.productId(tld) }
    }
}
