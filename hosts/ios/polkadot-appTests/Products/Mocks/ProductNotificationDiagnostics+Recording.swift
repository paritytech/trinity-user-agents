import Foundation
import IssueMonitoring
@testable import polkadot_app

extension ProductNotificationDiagnostics {
    /// Diagnostics whose events land in `recorder` with a fixed origin and clock.
    static func recording(
        into recorder: RecordingHostEventRecorder,
        issues: any IssueReporting = NoopIssueReporter()
    ) -> ProductNotificationDiagnostics {
        ProductNotificationDiagnostics(
            recorder: recorder,
            issues: issues,
            origin: GameReminderDiagnostics.testOrigin,
            monotonicMilliseconds: { GameReminderDiagnostics.testMonotonicMilliseconds }
        )
    }
}

/// The host event `ProductNotificationDiagnostics.recording` records for a step, spelled out field by field.
func expectedProductNotificationHostEvent(
    _ step: String,
    productId: String,
    id: UInt32?,
    deliveredAt: Date? = nil,
    scheduledAtMs: Int? = nil,
    level: HostLogLevel = .info,
    tags: [String: String] = [:]
) -> HostEvent {
    let expectedTags = [
        "host.app_variant": "test",
        "host.platform": "ios",
        "host.native_app_version": "1.2.3",
        "host.os_version": "26.5",
        "product_notification.product_id": productId
    ]

    var extras: [String: HostEventValue] = [
        "host.app_build": .integer(456),
        "event.monotonic_ms": .integer(GameReminderDiagnostics.testMonotonicMilliseconds),
        "app.launch_id": .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3")
    ]
    extras["product_notification.id"] = id.map { .integer(Int($0)) }
    extras["product_notification.delivered_at_unix_ms"] = deliveredAt.map {
        .integer(Int($0.timeIntervalSince1970 * 1_000))
    }
    extras["product_notification.scheduled_at_unix_ms"] = scheduledAtMs.map { .integer($0) }

    return HostEvent(
        name: "host.product_notification.\(step)",
        level: level,
        fingerprint: ["host.product_notification.\(step)"],
        tags: expectedTags.merging(tags) { _, new in new },
        extras: extras,
        counters: [:]
    )
}

/// Orders product notification events by booking id, for steps whose order among bookings is not defined.
func sortedByBookingId(_ events: some Sequence<HostEvent>) -> [HostEvent] {
    events.sorted {
        String(describing: $0.extras["product_notification.id"])
            < String(describing: $1.extras["product_notification.id"])
    }
}
