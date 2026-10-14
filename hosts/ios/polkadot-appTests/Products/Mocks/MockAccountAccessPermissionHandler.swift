import Foundation
import Products

final class MockAccountAccessPermissionHandler: AccountAccessPermissionHandling, @unchecked Sendable {
    var granted = true
    private(set) var recordedProductIds: [String] = []

    func request(productId: String, targetProductId _: String) async throws -> Bool {
        recordedProductIds.append(productId)
        return granted
    }
}
