import Foundation
import UserNotifications
@testable import polkadot_app

final class MockUserNotificationService: UserNotificationServicing, @unchecked Sendable {
    var accessStatus: NotificationAccessStatus = .allowed
    var scheduleError: Error?
    private(set) var scheduledIdentifiers: [String] = []

    func notificationAccessStatus() async -> NotificationAccessStatus {
        accessStatus
    }

    func requestNotificationsAuthorization(completion: ((Bool) -> Void)?) {
        completion?(accessStatus == .allowed)
    }

    func scheduleNotification(
        withIdentifier identifier: String,
        content _: UNNotificationContent,
        after _: TimeInterval,
        completion: ((Error?) -> Void)?
    ) {
        scheduledIdentifiers.append(identifier)
        completion?(scheduleError)
    }

    func cancelScheduledNotifications(withIdentifiers _: [String]) {}

    func isNotificationScheduled(withIdentifier identifier: String, completion: @escaping (Bool) -> Void) {
        completion(scheduledIdentifiers.contains(identifier))
    }

    func deliveredNotifications() async -> [UNNotification] {
        []
    }

    func removeDeliveredNotifications(withIdentifiers _: [String]) {}

    func setBadge(_: Int) async {}
}
