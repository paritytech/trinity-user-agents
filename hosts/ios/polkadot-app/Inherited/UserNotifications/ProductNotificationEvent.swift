import Foundation
import IssueMonitoring

/// A product notification the host showed or the user tapped. The event names the product and the
/// deeplink, never the notification's title or text.
struct ProductNotificationEvent: Equatable {
    enum Step: String {
        case shown
        case tapped
    }

    enum SeenVia: String {
        case foreground
        case nextLaunch = "next_launch"
    }

    let productId: String
    let deeplink: String?
    let deliveredAt: Date

    /// Nil unless the notification came from a product. Product notifications are scheduled with the
    /// identifier `product:<productId>:<notificationId>`.
    init?(identifier: String, userInfo: [AnyHashable: Any], deliveredAt: Date) {
        let prefix = "product:"
        guard userInfo[PushNotificationKeys.pushSource] as? Int == PushNotificationSource.products.rawValue,
              identifier.hasPrefix(prefix),
              let separator = identifier.lastIndex(of: ":"),
              separator >= identifier.index(identifier.startIndex, offsetBy: prefix.count)
        else {
            return nil
        }

        let productId = identifier[identifier.index(identifier.startIndex, offsetBy: prefix.count) ..< separator]
        guard !productId.isEmpty else {
            return nil
        }

        self.productId = String(productId)
        deeplink = userInfo[PushNotificationKeys.deeplink] as? String
        self.deliveredAt = deliveredAt
    }

    func hostEvent(
        step: Step,
        seenVia: SeenVia? = nil,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        var tags = [
            "host.os_version": origin.osVersion,
            "product_notification.product_id": productId
        ]
        tags["product_notification.seen_via"] = seenVia?.rawValue

        var extras = [
            "app.launch_id": HostEventValue.string(origin.launchID.uuidString.lowercased()),
            "product_notification.delivered_at_unix_ms": .integer(Int(deliveredAt.timeIntervalSince1970 * 1_000))
        ]
        extras["product_notification.deeplink"] = deeplink.map(HostEventValue.string)

        return .make(
            name: "host.product_notification.\(step.rawValue)",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags,
            extras: extras
        )
    }
}
