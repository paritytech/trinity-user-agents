import Foundation
import AlarmKit
import AppIntents
import IssueMonitoring
import Products
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
            let diagnostics = GameReminderDiagnostics(recorder: IssueMonitoringFactory.createHostEventRecorder())
            var tapped = GameReminderEvent(step: .tapped, productId: productId, startsAt: startsAt)
            tapped.appState = .init(UIApplication.shared.applicationState)
            diagnostics.record(tapped)

            let navigated = ProductOpener().open(productId: productId)
            var opened = GameReminderEvent(step: .productOpened, productId: productId, startsAt: startsAt)
            opened.openSource = .alarm
            opened.appState = .init(UIApplication.shared.applicationState)
            opened.navigated = navigated
            diagnostics.record(opened)
        }

        if let alarmIDString = alarmID,
           let alarmUUID = UUID(uuidString: alarmIDString) {
            try? AlarmManager.shared.stop(id: alarmUUID)
        }

        return .result()
    }
}
