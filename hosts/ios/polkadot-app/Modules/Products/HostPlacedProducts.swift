import Foundation
import Keystore_iOS
import Products
import TrUAPIHost

/// The products the host itself puts in chat: present on first run, with no add, install or discover
/// step, and removable by nobody. The chat analogue of Pocket's privileged card
/// (`docs/rfcs/pocket-modality.md`). The core decides which they are.
enum HostPlacedProducts {
    /// Whether the host places these is ``SettingsManagerProtocol/isHostPlacementEnabled``.
    static func all(tld: String) throws -> [HostPlacedChat] {
        try hostPlacedChats(networkSuffix: tld)
    }

    /// Read against the candidate's own TLD, so it answers without the chain read that listing
    /// needs — the trick Pocket's `pinned(key:)` uses. An exact match, not a prefix test, so
    /// `app.dim2.dot` is a different publisher and is not host-placed.
    static func contains(productId: ProductId) -> Bool {
        let labels = productId.rawValue.split(separator: ".")
        guard labels.count > 1, let tld = labels.last else {
            return false
        }

        return (try? all(tld: String(tld)))?.contains { $0.productId == productId } ?? false
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

        return (try? HostPlacedProducts.all(tld: tld))?.map(\.productId) ?? []
    }
}
