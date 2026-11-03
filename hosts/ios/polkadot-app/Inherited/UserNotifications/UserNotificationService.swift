import UserNotifications
import SDKLogger
import IssueMonitoring
import Keystore_iOS

protocol UserNotificationServicing {
    func notificationAccessStatus() async -> NotificationAccessStatus

    func requestNotificationsAuthorization(completion: ((Bool) -> Void)?)

    func scheduleNotification(
        withIdentifier identifier: String,
        content: UNNotificationContent,
        after timeInterval: TimeInterval,
        completion: ((Error?) -> Void)?
    )
    func cancelScheduledNotifications(withIdentifiers identifiers: [String])
    func isNotificationScheduled(withIdentifier identifier: String, completion: @escaping (Bool) -> Void)

    func deliveredNotifications() async -> [UNNotification]
    func removeDeliveredNotifications(withIdentifiers identifiers: [String])
    func setBadge(_ count: Int) async
}

extension UserNotificationServicing {
    func notificationAccessStatus(completion: @escaping (NotificationAccessStatus) -> Void) {
        Task {
            let notificationStatus = await self.notificationAccessStatus()

            await MainActor.run {
                completion(notificationStatus)
            }
        }
    }

    func scheduleNotification(
        withIdentifier identifier: String,
        content: UNNotificationContent,
        after timeInterval: TimeInterval
    ) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            self.scheduleNotification(
                withIdentifier: identifier,
                content: content,
                after: timeInterval
            ) { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            }
        }
    }

    func checkAccessAndScheduleNotification(
        withIdentifier identifier: String,
        content: UNNotificationContent,
        after timeInterval: TimeInterval
    ) async throws {
        let status = await notificationAccessStatus()

        switch status {
        case .allowed:
            try await scheduleNotification(
                withIdentifier: identifier,
                content: content,
                after: timeInterval
            )
        case .notAllowed:
            throw UserNotificationServiceError.notAuthorized
        }
    }

    func checkAccessAndScheduleNotificationNow(
        withIdentifier identifier: String,
        content: UNNotificationContent
    ) async throws {
        try await checkAccessAndScheduleNotification(
            withIdentifier: identifier,
            content: content,
            after: 0
        )
    }

    func checkAccessAndScheduleNotificationNow(
        withIdentifier identifier: String,
        title: String,
        message: String,
        source: PushNotificationSource
    ) async throws {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = message
        content.sound = .default

        content.userInfo[PushNotificationKeys.pushSource] = source.rawValue

        try await checkAccessAndScheduleNotificationNow(withIdentifier: identifier, content: content)
    }
}

enum UserNotificationServiceError: Error {
    case notAuthorized
}

/// Class is designed to handle push notifications:
///     1. Call startGatheringNotifications in application(_ application:didFinishLaunchingWithOptions _: ) to not miss
/// any notifications
///     2. Call setupHandlers when handlers are ready to handle notifications
///     3. Call activatePushNotificationsHandling when view hierarchy initialization finished and navigation can happen
final class UserNotificationService: NSObject {
    static let shared = UserNotificationService()

    private var pushTapHandler: PushNotificationTapHandling?
    private var foregroundPresentationDecider: PushForegroundPresentationDeciding?
    private var pendingResponses: [UNNotificationResponse] = []
    private var canHandlePushNotifications: Bool = false
    private let logger: LoggerProtocol
    private let recorder: any HostEventRecording
    private let settings: SettingsManagerProtocol
    private let origin: HostEventOrigin
    private let monotonicMilliseconds: @Sendable () -> Int
    private let isReporting: Bool

    let center: UNUserNotificationCenter = .current()

    init(
        logger: LoggerProtocol = Logger.shared,
        recorder: any HostEventRecording = IssueMonitoringFactory.createHostEventRecorder(),
        settings: SettingsManagerProtocol = SettingsManager.shared,
        origin: HostEventOrigin = .current,
        monotonicMilliseconds: @escaping @Sendable () -> Int = { HostEventClock.monotonicMilliseconds() },
        isReporting: Bool = IssueMonitoringFactory.isReportingSupported
    ) {
        self.logger = logger
        self.recorder = recorder
        self.settings = settings
        self.origin = origin
        self.monotonicMilliseconds = monotonicMilliseconds
        self.isReporting = isReporting
        super.init()
    }

    func startGatheringNotifications() {
        center.delegate = self
        reportNotificationsSeenSinceLastLaunch()
    }

    func setupHandlers(
        pushTapHandler: PushNotificationTapHandling?,
        foregroundPresentationDecider: PushForegroundPresentationDeciding?
    ) {
        self.pushTapHandler = pushTapHandler
        self.foregroundPresentationDecider = foregroundPresentationDecider

        guard let pushTapHandler, canHandlePushNotifications else {
            return
        }

        handlePendingNotification(pushTapHandler)
    }

    func activatePushNotificationsHandling() {
        canHandlePushNotifications = true

        guard let pushTapHandler else {
            return
        }

        handlePendingNotification(pushTapHandler)
    }
}

private extension UserNotificationService {
    /// Reports product notifications that arrived while the app was not in the foreground.
    func reportNotificationsSeenSinceLastLaunch() {
        guard isReporting else {
            return
        }
        let center = center
        let settings = settings
        Task {
            let now = Date()
            let delivered = await center.deliveredNotifications()
            let scanKey = SettingsKey.productNotificationsScannedThrough.rawValue
            let scannedThrough = settings.anyValue(for: scanKey) as? Date
            settings.set(anyValue: now, for: scanKey)

            for notification in delivered where ProductNotificationEvent.isNewSinceLastScan(
                deliveredAt: notification.date,
                scannedThrough: scannedThrough,
                now: now
            ) {
                record(notification, step: .shown, seenVia: .nextLaunch)
            }
        }
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

    func handlePendingNotification(_ pushTapHandler: PushNotificationTapHandling) {
        let responsesToReplay = pendingResponses
        pendingResponses.removeAll()

        for response in responsesToReplay {
            // we don't need to pass completion as it called
            // once response added to pending list
            pushTapHandler.handle(response: response, completion: {})
        }
    }
}

// MARK: - UserNotificationServicing

extension UserNotificationService: UserNotificationServicing {
    func notificationAccessStatus() async -> NotificationAccessStatus {
        let notificationSettings = await center.notificationSettings()
        return NotificationAccessStatus(status: notificationSettings.authorizationStatus)
    }

    func requestNotificationsAuthorization(completion: ((Bool) -> Void)?) {
        center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
            guard let completion else {
                return
            }
            DispatchQueue.main.async {
                completion(granted)
            }
        }
    }

    func scheduleNotification(
        withIdentifier identifier: String,
        content: UNNotificationContent,
        after timeInterval: TimeInterval,
        completion: ((Error?) -> Void)?
    ) {
        let trigger = UNTimeIntervalNotificationTrigger(
            timeInterval: max(1, timeInterval),
            repeats: false
        )

        let request = UNNotificationRequest(
            identifier: identifier,
            content: content,
            trigger: trigger
        )

        center.add(request, withCompletionHandler: completion)
    }

    func cancelScheduledNotifications(withIdentifiers identifiers: [String]) {
        center.removePendingNotificationRequests(withIdentifiers: identifiers)
    }

    func isNotificationScheduled(withIdentifier identifier: String, completion: @escaping (Bool) -> Void) {
        center.getPendingNotificationRequests { requests in
            var result = false

            for request in requests where request.identifier == identifier {
                result = true
                break
            }

            DispatchQueue.main.async {
                completion(result)
            }
        }
    }

    func deliveredNotifications() async -> [UNNotification] {
        await center.deliveredNotifications()
    }

    func removeDeliveredNotifications(withIdentifiers identifiers: [String]) {
        center.removeDeliveredNotifications(withIdentifiers: identifiers)
    }

    func setBadge(_ count: Int) async {
        do {
            try await center.setBadgeCount(count)
        } catch {
            logger.warning("Could not set badge count to \(count)")
        }
    }
}

// MARK: - UNUserNotificationCenterDelegate

extension UserNotificationService: UNUserNotificationCenterDelegate {
    func userNotificationCenter(
        _: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        let presentationOptions = foregroundPresentationDecider?.presentationOptions(for: notification) ?? []
        if !presentationOptions.isEmpty {
            record(notification, step: .shown, seenVia: .foreground)
        }
        completionHandler(presentationOptions)
    }

    func userNotificationCenter(
        _: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        record(response.notification, step: .tapped)
        guard canHandlePushNotifications, let pushTapHandler else {
            pendingResponses.append(response)
            completionHandler()
            return
        }

        pushTapHandler.handle(response: response, completion: completionHandler)
    }
}
