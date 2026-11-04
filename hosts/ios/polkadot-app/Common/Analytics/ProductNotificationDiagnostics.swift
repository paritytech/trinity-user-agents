import Foundation
import IssueMonitoring
import Keystore_iOS
import UserNotifications

/// Records the troubleshooting trail for product notifications through the shared host-event path.
struct ProductNotificationDiagnostics {
    private let recorder: any HostEventRecording
    private let settings: SettingsManagerProtocol
    private let origin: HostEventOrigin
    private let monotonicMilliseconds: @Sendable () -> Int
    private let now: @Sendable () -> Date
    private let isReporting: Bool

    init(
        recorder: any HostEventRecording = NoopHostEventRecorder(),
        settings: SettingsManagerProtocol = SettingsManager.shared,
        origin: HostEventOrigin = .current,
        monotonicMilliseconds: @escaping @Sendable () -> Int = { HostEventClock.monotonicMilliseconds() },
        now: @escaping @Sendable () -> Date = { Date() },
        isReporting: Bool = IssueMonitoringFactory.isReportingSupported
    ) {
        self.recorder = recorder
        self.settings = settings
        self.origin = origin
        self.monotonicMilliseconds = monotonicMilliseconds
        self.now = now
        self.isReporting = isReporting
    }

    /// Remembers the notification as reported so the next launch does not report it again.
    func shownInForeground(_ notification: UNNotification) {
        record(notification, step: .shown, seenVia: .foreground)
        let identifier = notification.request.identifier
        Task { @MainActor in
            markReported(identifier)
        }
    }

    func tapped(_ notification: UNNotification) {
        record(notification, step: .tapped)
    }

    /// Reports product notifications that arrived while the app was not in the foreground.
    /// `delivered` runs only in a build that reports, so the caller may put the Notification Center query behind it.
    /// The stored set is read and written on the main actor, like `markReported`, so they cannot clobber each other.
    func reportDeliveredSinceLastLaunch(_ delivered: @escaping @Sendable () async -> [UNNotification]) {
        guard isReporting else {
            return
        }
        Task { @MainActor in
            let scanTime = now()
            let before = reportedIdentifiers()
            let notifications = await delivered()
            let current = reportedIdentifiers() ?? []
            let newlyDelivered = ProductNotificationEvent.identifiersToReport(
                delivered: notifications.map { (identifier: $0.request.identifier, deliveredAt: $0.date) },
                alreadyReported: before,
                now: scanTime
            )
            let identifiersToReport = newlyDelivered.subtracting(current)
            let deliveredIdentifiers = Set(notifications.map(\.request.identifier))
            setReportedIdentifiers(deliveredIdentifiers.union(current.subtracting(before ?? [])))

            for notification in notifications where identifiersToReport.contains(notification.request.identifier) {
                record(notification, step: .shown, seenVia: .nextLaunch)
            }
        }
    }
}

private extension ProductNotificationDiagnostics {
    @MainActor
    func markReported(_ identifier: String) {
        guard isReporting else {
            return
        }
        setReportedIdentifiers((reportedIdentifiers() ?? []).union([identifier]))
    }

    /// nil means nothing has been stored yet, which is the first scan.
    func reportedIdentifiers() -> Set<String>? {
        let stored = settings.anyValue(for: SettingsKey.productNotificationsReported.rawValue)
        return (stored as? [String]).map(Set.init)
    }

    func setReportedIdentifiers(_ identifiers: Set<String>) {
        settings.set(anyValue: Array(identifiers), for: SettingsKey.productNotificationsReported.rawValue)
    }

    func record(
        _ notification: UNNotification,
        step: ProductNotificationEvent.Step,
        seenVia: ProductNotificationEvent.SeenVia? = nil
    ) {
        guard let event = ProductNotificationEvent(
            identifier: notification.request.identifier,
            userInfo: notification.request.content.userInfo,
            deliveredAt: notification.date
        ) else {
            return
        }

        recorder.record(
            event.hostEvent(
                step: step,
                seenVia: seenVia,
                origin: origin,
                monotonicMilliseconds: monotonicMilliseconds()
            )
        )
    }
}
