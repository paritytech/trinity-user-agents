import Foundation
import IssueMonitoring
@testable import polkadot_app

extension GameReminderDiagnostics {
    static let testOrigin = HostEventOrigin(
        launchID: UUID(uuidString: "D93FE846-8F3C-4CFA-A440-402B6F7B43D3")!,
        osVersion: "26.5",
        appVersion: "1.2.3",
        appBuild: 456,
        appVariant: "test"
    )
    static let testMonotonicMilliseconds = 7

    /// Diagnostics whose events land in `recorder` with a fixed origin and clock.
    static func recording(into recorder: RecordingHostEventRecorder) -> GameReminderDiagnostics {
        GameReminderDiagnostics(
            recorder: recorder,
            origin: testOrigin,
            monotonicMilliseconds: { testMonotonicMilliseconds }
        )
    }
}

/// The host event `GameReminderDiagnostics.recording` records for a step, spelled out field by field.
func expectedReminderHostEvent(
    _ step: String,
    productId: String,
    startsAt: Date,
    occurredAt: Date,
    tags: [String: String] = [:]
) -> HostEvent {
    HostEvent(
        name: "host.reminder.\(step)",
        tags: [
            "host.app_variant": "test",
            "host.platform": "ios",
            "host.native_app_version": "1.2.3",
            "host.os_version": "26.5",
            "reminder.product_id": productId
        ].merging(tags) { _, new in new },
        extras: [
            "host.app_build": .integer(456),
            "event.monotonic_ms": .integer(GameReminderDiagnostics.testMonotonicMilliseconds),
            "app.launch_id": .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"),
            "reminder.starts_at_unix_ms": .integer(Int(startsAt.timeIntervalSince1970 * 1_000)),
            "reminder.delay_after_start_ms": .integer(Int(occurredAt.timeIntervalSince(startsAt) * 1_000))
        ]
    )
}
