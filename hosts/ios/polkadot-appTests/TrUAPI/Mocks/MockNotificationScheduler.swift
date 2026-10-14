import Foundation
import Products
@testable import polkadot_app

final class MockNotificationScheduler: ProductNotificationScheduling, @unchecked Sendable {
    var scheduledProductId: String?
    var scheduledRequest: ScheduledNotificationRequest?
    var notificationIdToReturn: UInt32 = 42
    var cancelledNotificationId: UInt32?
    var onCancel: ((UInt32) -> Void)?

    func schedule(productId: ProductId, request: ScheduledNotificationRequest) async throws -> UInt32 {
        scheduledProductId = productId
        scheduledRequest = request
        return notificationIdToReturn
    }

    func cancel(productId _: ProductId, notificationId: UInt32) async throws {
        cancelledNotificationId = notificationId
        onCancel?(notificationId)
    }

    func cancelAll(forProductId _: ProductId) async throws {}
}
