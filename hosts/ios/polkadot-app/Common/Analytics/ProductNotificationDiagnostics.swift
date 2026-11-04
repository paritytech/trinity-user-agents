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
        markReported(notification.request.identifier)
    }

    func tapped(_ notification: UNNotification) {
        record(notification, step: .tapped)
    }

    /// Reports product notifications that arrived while the app was not in the foreground.
    /// `delivered` runs only in a build that reports, so the caller may put the Notification Center query behind it.
    func reportDeliveredSinceLastLaunch(_ delivered: @escaping @Sendable () async -> [UNNotification]) {
        guard isReporting else {
            return
        }
        Task {
            let scanTime = now()
            let delivered = await delivered()
            let key = SettingsKey.productNotificationsReported.rawValue
            let alreadyReported = (settings.anyValue(for: key) as? [String]).map(Set.init)
            let identifiersToReport = ProductNotificationEvent.identifiersToReport(
                delivered: delivered.map { (identifier: $0.request.identifier, deliveredAt: $0.date) },
                alreadyReported: alreadyReported,
                now: scanTime
            )
            settings.set(anyValue: delivered.map(\.request.identifier), for: key)

            for notification in delivered where identifiersToReport.contains(notification.request.identifier) {
                record(notification, step: .shown, seenVia: .nextLaunch)
            }
        }
    }
}

private extension ProductNotificationDiagnostics {
    func markReported(_ identifier: String) {
        guard isReporting else {
            return
        }
        let key = SettingsKey.productNotificationsReported.rawValue
        var identifiers = settings.anyValue(for: key) as? [String] ?? []
        if !identifiers.contains(identifier) {
            identifiers.append(identifier)
        }
        settings.set(anyValue: identifiers, for: key)
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
