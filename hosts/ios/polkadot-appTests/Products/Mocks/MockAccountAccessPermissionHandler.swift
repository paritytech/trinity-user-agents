import struct TrUAPIHost.ProductId
import Foundation
import Products

final class MockAccountAccessPermissionHandler: AccountAccessPermissionHandling, @unchecked Sendable {
    var granted = true
    private(set) var recordedProductIds: [ProductId] = []

    func request(productId: ProductId, targetProductId _: ProductId) async throws -> Bool {
        recordedProductIds.append(productId)
        return granted
    }
}
