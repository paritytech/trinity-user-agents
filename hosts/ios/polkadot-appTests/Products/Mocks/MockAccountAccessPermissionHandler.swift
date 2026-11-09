import struct TrUAPIHost.ProductId
import Foundation
import Products

final class MockAccountAccessPermissionHandler: AccountAccessPermissionHandling, @unchecked Sendable {
    var granted = true
    private(set) var recordedProductIds: [ProductId] = []
    private(set) var recordedTargetProductIds: [ProductId] = []

    func request(productId: ProductId, targetProductId: ProductId) async throws -> Bool {
        recordedProductIds.append(productId)
        recordedTargetProductIds.append(targetProductId)
        return granted
    }
}
