import Foundation
import AlarmKit
import AppIntents
import IssueMonitoring
import UIKit

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
            let telemetry = IssueMonitoringFactory.createGameReminderTelemetry()
            let appState = GameReminderEvent.AppState(UIApplication.shared.applicationState)

            var tapped = GameReminderEvent(step: .tapped, productId: productId, startsAt: startsAt)
            tapped.delivery = .alarm
            tapped.appState = appState
            telemetry.record(tapped)

            var opened = GameReminderEvent(step: .productOpened, productId: productId, startsAt: startsAt)
            opened.openSource = .alarm
            opened.appState = appState
            opened.navigated = ProductOpener().open(productId: productId)
            telemetry.record(opened)
        }

        if let alarmIDString = alarmID,
           let alarmUUID = UUID(uuidString: alarmIDString) {
            try? AlarmManager.shared.stop(id: alarmUUID)
        }

        return .result()
    }
}
