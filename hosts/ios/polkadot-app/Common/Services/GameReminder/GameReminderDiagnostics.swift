import Foundation
import IssueMonitoring
import Products
import UIKit

/// Records the troubleshooting trail for a product's game reminder through the shared host-event path.
struct GameReminderDiagnostics: Sendable {
    private let recorder: any HostEventRecording
    private let origin: HostEventOrigin
    private let monotonicMilliseconds: @Sendable () -> Int
    private let now: @Sendable () -> Date
    private let applicationState: @MainActor @Sendable () -> UIApplication.State

    init(
        recorder: any HostEventRecording = NoopHostEventRecorder(),
        origin: HostEventOrigin = .current,
        monotonicMilliseconds: @escaping @Sendable () -> Int = { HostEventClock.monotonicMilliseconds() },
        now: @escaping @Sendable () -> Date = { Date() },
        applicationState: @escaping @MainActor @Sendable () -> UIApplication.State = {
            UIApplication.shared.applicationState
        }
    ) {
        self.recorder = recorder
        self.origin = origin
        self.monotonicMilliseconds = monotonicMilliseconds
        self.now = now
        self.applicationState = applicationState
    }

    func scheduled(productId: ProductId, startsAt: Date, delivery: GameReminderEvent.Delivery) {
        record(
            GameReminderEvent(
                step: .scheduled,
                productId: productId,
                startsAt: startsAt,
                occurredAt: now(),
                delivery: delivery
            )
        )
    }

    func notScheduled(productId: ProductId, startsAt: Date, reason: GameReminderEvent.SkipReason) {
        record(
            GameReminderEvent(
                step: .notScheduled,
                productId: productId,
                startsAt: startsAt,
                occurredAt: now(),
                skipReason: reason
            )
        )
    }

    /// Only AlarmKit reports a ring, so the delivery is always the alarm.
    @MainActor
    func fired(productId: ProductId, startsAt: Date) {
        record(
            GameReminderEvent(
                step: .fired,
                productId: productId,
                startsAt: startsAt,
                occurredAt: now(),
                delivery: .alarm,
                appState: .init(applicationState())
            )
        )
    }

    @MainActor
    func cancelled(productId: ProductId, startsAt: Date) {
        record(
            GameReminderEvent(
                step: .cancelled,
                productId: productId,
                startsAt: startsAt,
                occurredAt: now(),
                appState: .init(applicationState())
            )
        )
    }

    @MainActor
    func productOpened(productId: ProductId, startsAt: @autoclosure () -> Date?, source: GameReminderEvent.OpenSource) {
        record(
            GameReminderEvent(
                step: .productOpened,
                productId: productId,
                startsAt: startsAt(),
                occurredAt: now(),
                openSource: source,
                appState: .init(applicationState())
            )
        )
    }

    @MainActor
    func tapped(productId: ProductId, startsAt: @autoclosure () -> Date?) {
        record(
            GameReminderEvent(
                step: .tapped,
                productId: productId,
                startsAt: startsAt(),
                occurredAt: now(),
                appState: .init(applicationState())
            )
        )
    }

    /// The event is built only when the recorder keeps it.
    private func record(_ event: @autoclosure () -> GameReminderEvent) {
        recorder.record(event().hostEvent(origin: origin, monotonicMilliseconds: monotonicMilliseconds()))
    }
}
