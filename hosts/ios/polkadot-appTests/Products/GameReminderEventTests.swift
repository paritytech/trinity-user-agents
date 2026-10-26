import Foundation
import IssueMonitoring
import Products
import Testing

struct GameReminderEventTests {
    private let startsAt = Date(timeIntervalSince1970: 2_000_000_000)
    private let origin = HostEventOrigin(
        launchID: UUID(uuidString: "D93FE846-8F3C-4CFA-A440-402B6F7B43D3")!,
        osVersion: "26.5",
        appVersion: "1.2.3",
        appBuild: 456,
        appVariant: "nightly"
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

        let hostEvent = event.hostEvent(origin: origin)

        #expect(hostEvent.name == "host.reminder.product_opened")
        #expect(hostEvent.tags["reminder.product_id"] == "game.dot")
        #expect(hostEvent.tags["reminder.open_source"] == "alarm")
        #expect(hostEvent.tags["host.app_state"] == "inactive")
        #expect(hostEvent.extras["reminder.starts_at_unix_ms"] == .integer(2_000_000_000_000))
        #expect(hostEvent.extras["reminder.delay_after_start_ms"] == .integer(1_500))
        #expect(hostEvent.extras["reminder.navigated"] == .boolean(true))
    }

    @Test func eventCarriesTheCanonicalHostFields() {
        let hostEvent = GameReminderEvent(step: .scheduled, productId: "game.dot", startsAt: startsAt)
            .hostEvent(origin: origin)

        #expect(hostEvent.tags["host.platform"] == "ios")
        #expect(hostEvent.tags["host.os_version"] == "26.5")
        #expect(hostEvent.tags["host.native_app_version"] == "1.2.3")
        #expect(hostEvent.tags["host.app_variant"] == "nightly")
        #expect(hostEvent.extras["host.app_build"] == .integer(456))
        #expect(hostEvent.extras["app.launch_id"] == .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))
    }

    @Test func skippedScheduleNamesItsDeliveryAndReason() {
        var event = GameReminderEvent(step: .notScheduled, productId: "game.dot", startsAt: startsAt)
        event.delivery = .alarm
        event.skipReason = .notAuthorized

        let hostEvent = event.hostEvent(origin: origin)

        #expect(hostEvent.name == "host.reminder.not_scheduled")
        #expect(hostEvent.tags["reminder.delivery"] == "alarm")
        #expect(hostEvent.tags["reminder.skip_reason"] == "not_authorized")
    }

    @Test func absentValuesAreOmittedInsteadOfInvented() {
        let hostEvent = GameReminderEvent(step: .tapped, productId: "game.dot", startsAt: nil)
            .hostEvent(origin: HostEventOrigin(
                launchID: UUID(),
                osVersion: "26.5",
                appVersion: nil,
                appBuild: nil,
                appVariant: "release"
            ))

        #expect(hostEvent.extras["reminder.starts_at_unix_ms"] == nil)
        #expect(hostEvent.extras["reminder.delay_after_start_ms"] == nil)
        #expect(hostEvent.extras["reminder.navigated"] == nil)
        #expect(hostEvent.extras["host.app_build"] == nil)
        #expect(hostEvent.tags["host.native_app_version"] == nil)
        #expect(hostEvent.tags["reminder.delivery"] == nil)
        #expect(hostEvent.tags["reminder.open_source"] == nil)
    }
}
