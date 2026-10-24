import Foundation
import IssueMonitoring
import Testing

struct GameReminderEventTests {
    private let startsAt = Date(timeIntervalSince1970: 2_000_000_000)
    private let origin = HostEventOrigin(
        launchID: UUID(uuidString: "D93FE846-8F3C-4CFA-A440-402B6F7B43D3")!,
        osVersion: "26.5",
        appVersion: "1.2.3",
        appBuild: 456
    )

    @Test func eventCarriesTheProductTheStartAndHowLateItCameAsNumbers() {
        var event = GameReminderEvent(
            step: .productOpened,
            productId: "game.dot",
            startsAt: startsAt,
            occurredAt: startsAt.addingTimeInterval(1.5)
        )
        event.openSource = .alarm
        event.appState = .inactive
        event.navigated = true

        let fields = event.sentryFields(origin: origin)

        #expect(event.name == "host.reminder.product_opened")
        #expect(GameReminderEvent.logger == "host.telemetry")
        #expect(fields.tags["reminder.product_id"] == "game.dot")
        #expect(fields.tags["reminder.open_source"] == "alarm")
        #expect(fields.tags["host.app_state"] == "inactive")
        #expect(fields.extras["reminder.starts_at_unix_ms"] as? Int64 == 2_000_000_000_000)
        #expect(fields.extras["reminder.delay_after_start_ms"] as? Int64 == 1_500)
        #expect(fields.extras["reminder.navigated"] as? Bool == true)
    }

    @Test func eventCarriesTheCanonicalHostFields() {
        let fields = GameReminderEvent(step: .scheduled, productId: "game.dot", startsAt: startsAt)
            .sentryFields(origin: origin)

        #expect(fields.tags["host.platform"] == "ios")
        #expect(fields.tags["host.os_version"] == "26.5")
        #expect(fields.tags["host.native_app_version"] == "1.2.3")
        #expect(fields.extras["host.app_build"] as? Int == 456)
        #expect(fields.extras["app.launch_id"] as? String == "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")
    }

    @Test func skippedScheduleNamesItsDeliveryAndReason() {
        var event = GameReminderEvent(step: .notScheduled, productId: "game.dot", startsAt: startsAt)
        event.delivery = .alarm
        event.skipReason = .notAuthorized

        let fields = event.sentryFields(origin: origin)

        #expect(event.name == "host.reminder.not_scheduled")
        #expect(fields.tags["reminder.delivery"] == "alarm")
        #expect(fields.tags["reminder.skip_reason"] == "not_authorized")
    }

    @Test func absentValuesAreOmittedInsteadOfInvented() {
        let fields = GameReminderEvent(step: .tapped, productId: "game.dot", startsAt: nil)
            .sentryFields(origin: HostEventOrigin(launchID: UUID(), osVersion: "26.5", appVersion: nil, appBuild: nil))

        #expect(fields.extras["reminder.starts_at_unix_ms"] == nil)
        #expect(fields.extras["reminder.delay_after_start_ms"] == nil)
        #expect(fields.extras["reminder.navigated"] == nil)
        #expect(fields.extras["host.app_build"] == nil)
        #expect(fields.tags["host.native_app_version"] == nil)
        #expect(fields.tags["reminder.delivery"] == nil)
        #expect(fields.tags["reminder.open_source"] == nil)
    }
}
