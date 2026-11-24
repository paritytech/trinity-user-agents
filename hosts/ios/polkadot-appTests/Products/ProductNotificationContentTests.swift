import Foundation
import Products
import Testing
import struct TrUAPIHost.ProductId
@testable import polkadot_app

@Suite("Product notification content")
struct ProductNotificationContentTests {
    private let productId = try! ProductId("dim2.dot")
    private let request = ScheduledNotificationRequest(
        text: "Your scratch card is ready",
        deeplink: nil,
        scheduledAtMs: nil
    )

    /// The core only lets a trusted product's title through, so the scheduler
    /// shows it in place of the product id.
    @Test func showsTheTitleTheCoreLetThrough() {
        let content = ProductNotificationScheduler.makeContent(
            productId: productId,
            title: "Daily Dollar",
            request: request
        )

        #expect(content.title == "Daily Dollar")
        #expect(content.body == "Your scratch card is ready")
    }

    /// Without a title the product id is the only attribution the person sees.
    @Test func fallsBackToTheProductId() {
        let content = ProductNotificationScheduler.makeContent(productId: productId, title: nil, request: request)

        #expect(content.title == "dim2.dot")
    }
}
