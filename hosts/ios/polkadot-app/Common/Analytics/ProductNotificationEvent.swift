import Foundation
import IssueMonitoring

/// A product notification the host showed or the user tapped. The event names only the product: the deeplink is
/// chosen by the product and can carry ids or keys, so it is never sent, nor is the notification's title or text.
struct ProductNotificationEvent: Equatable {
    enum Step: String {
        case shown
        case tapped
    }

    let productId: String
    let deliveredAt: Date

    /// Nil unless the notification came from a product. Product notifications are scheduled with the
    /// identifier `product:<productId>:<notificationId>`.
    init?(identifier: String, userInfo: [AnyHashable: Any], deliveredAt: Date) {
        let prefix = "product:"
        let rest = identifier.dropFirst(prefix.count)
        guard userInfo[PushNotificationKeys.pushSource] as? Int == PushNotificationSource.products.rawValue,
              identifier.hasPrefix(prefix),
              let separator = rest.lastIndex(of: ":"),
              !rest[..<separator].isEmpty
        else {
            return nil
        }

        productId = String(rest[..<separator])
        self.deliveredAt = deliveredAt
    }

    func hostEvent(
        step: Step,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        let tags = [
            "host.os_version": origin.osVersion,
            "product_notification.product_id": productId
        ]
        let extras = [
            "app.launch_id": HostEventValue.string(origin.launchID.uuidString.lowercased()),
            "product_notification.delivered_at_unix_ms": .integer(Int(deliveredAt.timeIntervalSince1970 * 1_000))
        ]

        return .make(
            name: "host.product_notification.\(step.rawValue)",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags,
            extras: extras
        )
    }
}
