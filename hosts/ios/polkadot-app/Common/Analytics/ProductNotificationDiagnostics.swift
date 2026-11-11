import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring

/// Records the troubleshooting trail for product notifications through the shared host-event path.
struct ProductNotificationDiagnostics: Sendable {
    private let recorder: any HostEventRecording
    private let issues: any IssueReporting
    private let origin: HostEventOrigin
    private let monotonicMilliseconds: @Sendable () -> Int

    init(
        recorder: any HostEventRecording = NoopHostEventRecorder(),
        issues: any IssueReporting = NoopIssueReporter(),
        origin: HostEventOrigin = .current,
        monotonicMilliseconds: @escaping @Sendable () -> Int = { HostEventClock.monotonicMilliseconds() }
    ) {
        self.recorder = recorder
        self.issues = issues
        self.origin = origin
        self.monotonicMilliseconds = monotonicMilliseconds
    }

    func scheduled(entry: ScheduledNotificationEntry, scheduledAtMs: UInt64? = nil) {
        record(.scheduled, event: ProductNotificationEvent(entry: entry, scheduledAtMs: scheduledAtMs))
    }

    func notScheduled(
        productId: ProductId,
        notificationId: UInt32?,
        reason: ProductNotificationEvent.Reason,
        error: Error? = nil
    ) {
        record(
            .notScheduled,
            event: ProductNotificationEvent(productId: productId, notificationId: notificationId, reason: reason)
        )
        switch reason {
        case .addFailed:
            issues.report(CriticalIssue(flow: "product-notification", kind: "add-failed", error: error))
        case .storeFailed:
            issues.report(CriticalIssue(flow: "product-notification", kind: "store-failed", error: error))
        case .limitReached:
            break
        }
    }

    func cancelled(entry: ScheduledNotificationEntry) {
        record(.cancelled, event: ProductNotificationEvent(entry: entry))
    }

    func delivered(entry: ScheduledNotificationEntry, deliveredAt: Date) {
        record(.delivered, event: ProductNotificationEvent(entry: entry, deliveredAt: deliveredAt))
    }

    func gone(entry: ScheduledNotificationEntry) {
        record(.gone, event: ProductNotificationEvent(entry: entry))
    }

    func shown(identifier: String, userInfo: [AnyHashable: Any], deliveredAt: Date) {
        record(.shown, identifier: identifier, userInfo: userInfo, deliveredAt: deliveredAt)
    }

    func suppressed(identifier: String, userInfo: [AnyHashable: Any], deliveredAt: Date) {
        record(.suppressed, identifier: identifier, userInfo: userInfo, deliveredAt: deliveredAt)
    }

    func tapped(identifier: String, userInfo: [AnyHashable: Any], deliveredAt: Date) {
        record(.tapped, identifier: identifier, userInfo: userInfo, deliveredAt: deliveredAt)
    }
}

private extension ProductNotificationDiagnostics {
    /// Only a notification a product scheduled produces an event.
    func record(
        _ step: ProductNotificationEvent.Step,
        identifier: String,
        userInfo: [AnyHashable: Any],
        deliveredAt: Date
    ) {
        guard let event = ProductNotificationEvent(identifier: identifier, userInfo: userInfo, deliveredAt: deliveredAt)
        else {
            return
        }
        record(step, event: event)
    }

    func record(_ step: ProductNotificationEvent.Step, event: ProductNotificationEvent) {
        recorder.record(
            event.hostEvent(step: step, origin: origin, monotonicMilliseconds: monotonicMilliseconds())
        )
    }
}

private extension ProductNotificationEvent {
    init(entry: ScheduledNotificationEntry, deliveredAt: Date? = nil, scheduledAtMs: UInt64? = nil) {
        self.init(
            productId: entry.productId,
            notificationId: entry.notificationId,
            deliveredAt: deliveredAt,
            scheduledAtMs: scheduledAtMs
        )
    }
}
