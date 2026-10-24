import Foundation
import AlarmKit
import AppIntents

@available(iOS 26.1, *)
struct GameAlarmPlayIntent: LiveActivityIntent {
    static var title: LocalizedStringResource = "Join"
    static var openAppWhenRun: Bool = true

    @Parameter(title: "Alarm ID")
    var alarmID: String?

    @Parameter(title: "Product ID")
    var productId: String?

    @Parameter(title: "Starts At")
    var startsAt: Date?

    @MainActor
    func perform() async throws -> some IntentResult {
        if let productId {
            // A tap that launches the app can come before the services are up; the center then works on the
            // same stored reminders.
            let registered: ProductGameReminderScheduling? = RootDependencyLocator.getDependency()
            let reminders = registered as? ProductGameReminderCenter ?? ProductGameReminderCenter.makeDefault()
            reminders.openFromAlarm(productId: productId, startsAt: startsAt)
        }

        if let alarmIDString = alarmID,
           let alarmUUID = UUID(uuidString: alarmIDString) {
            try? AlarmManager.shared.stop(id: alarmUUID)
        }

        return .result()
    }
}
