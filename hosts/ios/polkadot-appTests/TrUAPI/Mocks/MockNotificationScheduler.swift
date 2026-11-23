import struct TrUAPIHost.ProductId
import Foundation
import Products
@testable import polkadot_app

final class MockNotificationScheduler: ProductNotificationScheduling, @unchecked Sendable {
    var scheduledProductId: ProductId?
    var scheduledTitle: String?
    var scheduledRequest: ScheduledNotificationRequest?
    var notificationIdToReturn: UInt32 = 42
    var cancelledNotificationId: UInt32?
    var onCancel: ((UInt32) -> Void)?

    func schedule(
        productId: ProductId,
        title: String?,
        request: ScheduledNotificationRequest
    ) async throws -> UInt32 {
        scheduledProductId = productId
        scheduledTitle = title
        scheduledRequest = request
        return notificationIdToReturn
    }

    func cancel(productId _: ProductId, notificationId: UInt32) async throws {
        cancelledNotificationId = notificationId
        onCancel?(notificationId)
    }

    func cancelAll(forProductId _: ProductId) async throws {}
}
