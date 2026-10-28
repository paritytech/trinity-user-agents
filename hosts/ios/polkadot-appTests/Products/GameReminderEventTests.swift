import Foundation
import IssueMonitoring
import Testing
@testable import polkadot_app

struct GameReminderEventTests {
    private let startsAt = Date(timeIntervalSince1970: 2_000_000_000)

    @Test func eventCarriesTheProductTheStartAndHowLateItCameAsNumbers() {
        let event = GameReminderEvent(
            step: .productOpened,
            productId: "game.dot",
            startsAt: startsAt,
            occurredAt: startsAt.addingTimeInterval(1.5),
            openSource: .alarm,
            appState: .inactive
        )

        #expect(
            event.hostEvent(origin: GameReminderDiagnostics.testOrigin, monotonicMilliseconds: 7)
                == HostEvent(
                    name: "host.reminder.product_opened",
                    tags: [
                        "host.app_variant": "test",
                        "host.platform": "ios",
                        "host.native_app_version": "1.2.3",
                        "host.os_version": "26.5",
                        "reminder.product_id": "game.dot",
                        "reminder.open_source": "alarm",
                        "host.app_state": "inactive"
                    ],
                    extras: [
                        "host.app_build": .integer(456),
                        "event.monotonic_ms": .integer(7),
                        "app.launch_id": .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"),
                        "reminder.starts_at_unix_ms": .integer(2_000_000_000_000),
                        "reminder.delay_after_start_ms": .integer(1_500)
                    ]
                )
        )
    }

    @Test func absentValuesAreOmittedInsteadOfInvented() {
        let origin = HostEventOrigin(
            launchID: UUID(uuidString: "D93FE846-8F3C-4CFA-A440-402B6F7B43D3")!,
            osVersion: "26.5",
            appVersion: nil,
            appBuild: nil,
            appVariant: "release"
        )

        #expect(
            GameReminderEvent(step: .tapped, productId: "game.dot", startsAt: nil)
                .hostEvent(origin: origin, monotonicMilliseconds: 7)
                == HostEvent(
                    name: "host.reminder.tapped",
                    tags: [
                        "host.app_variant": "release",
                        "host.platform": "ios",
                        "host.os_version": "26.5",
                        "reminder.product_id": "game.dot"
                    ],
                    extras: [
                        "event.monotonic_ms": .integer(7),
                        "app.launch_id": .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3")
                    ]
                )
        )
    }

    @Test func aNoopRecorderNeverBuildsTheEvent() {
        var built = false

        GameReminderDiagnostics().record({
            built = true
            return GameReminderEvent(step: .tapped, productId: "game.dot", startsAt: nil)
        }())

        #expect(!built)
    }
}
