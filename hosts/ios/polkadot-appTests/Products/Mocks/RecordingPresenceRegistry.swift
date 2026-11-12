import Foundation
import struct TrUAPIHost.ProductId
@testable import polkadot_app

@MainActor
final class RecordingPresenceRegistry: ProductPresenceRegistering {
    struct Registration: Equatable {
        let id: UUID
        let productId: ProductId
        let contentId: ProductId
        let displayName: String
    }

    private(set) var registrations: [Registration] = []
    private(set) var unregisteredIds: [UUID] = []
    private(set) var refreshedProductIds: [ProductId] = []

    func register(
        productId: ProductId,
        contentId: ProductId,
        displayName: String,
        reloader _: SPAProductReloading
    ) -> UUID {
        let id = UUID()
        registrations.append(Registration(id: id, productId: productId, contentId: contentId, displayName: displayName))
        return id
    }

    func unregister(_ presenceId: UUID) {
        unregisteredIds.append(presenceId)
    }

    func requestRefresh(of productId: ProductId) {
        refreshedProductIds.append(productId)
    }

    func applyPendingUpdate(of _: ProductId) -> Bool {
        false
    }
}
