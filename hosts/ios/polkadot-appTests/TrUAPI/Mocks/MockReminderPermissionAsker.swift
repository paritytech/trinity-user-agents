import Foundation
@testable import polkadot_app

final class MockReminderPermissionAsker: ReminderPermissionAsking, @unchecked Sendable {
    enum Ask: Equatable {
        case alarm
        case notifications
    }

    var alarmAllowed = false
    var notificationsAllowed = false
    private(set) var asked: [Ask] = []

    func askAlarm() async -> Bool {
        asked.append(.alarm)
        return alarmAllowed
    }

    func askNotifications() async -> Bool {
        asked.append(.notifications)
        return notificationsAllowed
    }
}
