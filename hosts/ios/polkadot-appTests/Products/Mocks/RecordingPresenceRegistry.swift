import Foundation
import struct TrUAPIHost.ProductId
@testable import polkadot_app

@MainActor
final class RecordingPresenceRegistry: ProductPresenceRegistering {
    private(set) var registeredContentIds: [ProductId] = []

    func register(
        productId _: ProductId,
        contentId: ProductId,
        displayName _: String,
        reloader _: SPAProductReloading
    ) -> UUID {
        registeredContentIds.append(contentId)
        return UUID()
    }

    func unregister(_: UUID) {}

    func requestRefresh(of _: ProductId) {}

    func applyPendingUpdate(of _: ProductId) -> Bool {
        false
    }
}
