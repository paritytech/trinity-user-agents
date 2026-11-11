import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring

/// A step in the life of a product notification. The event names only the product and the booking id: the deeplink
/// is chosen by the product and can carry ids or keys, so it is never sent, nor is the title or text of the
/// notification.
struct ProductNotificationEvent: Equatable {
    enum Step: String {
        /// The system accepted the notification and the host stored it.
        case scheduled
        /// The booking failed, and the event carries the reason.
        case notScheduled = "not_scheduled"
        /// The notification fired in the foreground and the host displayed it.
        case shown
        /// The notification fired in the foreground and the host kept it off the screen.
        case suppressed
        /// The notification left the pending list and is still in Notification Center.
        case delivered
        /// The notification is neither pending nor in Notification Center. On its own this does not mean it was lost:
        /// a banner shown in the foreground, a tapped one and one the user swiped away all read as gone at the next
        /// sweep.
        case gone
        /// The host cancelled the booking.
        case cancelled
        /// The user tapped the notification.
        case tapped
    }

    /// Why a booking did not reach the system and the store.
    enum Reason: String {
        case limitReached = "limit_reached"
        case sweepFailed = "sweep_failed"
        case readFailed = "read_failed"
        case addFailed = "add_failed"
        case storeFailed = "store_failed"
    }

    let productId: String
    let notificationId: UInt32?
    let deliveredAt: Date?
    let scheduledAtMs: UInt64?
    let reason: Reason?

    /// Nil unless the notification came from a product. Product notifications are scheduled with the
    /// identifier `product:<productId>:<notificationId>`.
    init?(identifier: String, userInfo: [AnyHashable: Any], deliveredAt: Date) {
        let prefix = "product:"
        let rest = identifier.dropFirst(prefix.count)
        guard userInfo[PushNotificationKeys.pushSource] as? Int == PushNotificationSource.products.rawValue,
              identifier.hasPrefix(prefix),
              let separator = rest.lastIndex(of: ":"),
              !rest[..<separator].isEmpty,
              let notificationId = UInt32(rest[rest.index(after: separator)...])
        else {
            return nil
        }

        productId = String(rest[..<separator])
        self.notificationId = notificationId
        self.deliveredAt = deliveredAt
        scheduledAtMs = nil
        reason = nil
    }

    init(
        productId: ProductId,
        notificationId: UInt32?,
        deliveredAt: Date? = nil,
        scheduledAtMs: UInt64? = nil,
        reason: Reason? = nil
    ) {
        self.productId = productId.rawValue
        self.notificationId = notificationId
        self.deliveredAt = deliveredAt
        self.scheduledAtMs = scheduledAtMs
        self.reason = reason
    }

    func hostEvent(
        step: Step,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        var tags = [
            "host.os_version": origin.osVersion,
            "product_notification.product_id": productId
        ]
        tags["product_notification.reason"] = reason?.rawValue

        var extras = ["app.launch_id": HostEventValue.string(origin.launchID.uuidString.lowercased())]
        extras["product_notification.id"] = notificationId.map { .integer(Int($0)) }
        if let deliveredAt {
            let deliveredAtMs = Int(deliveredAt.timeIntervalSince1970 * 1_000)
            extras["product_notification.delivered_at_unix_ms"] = .integer(deliveredAtMs)
        }
        if let scheduledAtMs {
            extras["product_notification.scheduled_at_unix_ms"] = .integer(Int(clamping: scheduledAtMs))
        }

        return .make(
            name: "host.product_notification.\(step.rawValue)",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            level: reason == nil || reason == .limitReached ? .info : .error,
            tags: tags,
            extras: extras
        )
    }
}
