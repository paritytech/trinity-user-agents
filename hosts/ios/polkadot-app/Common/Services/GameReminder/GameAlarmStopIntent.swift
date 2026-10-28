import Foundation
import AlarmKit
import AppIntents
import IssueMonitoring
import Keystore_iOS
import UIKit

@available(iOS 26.1, *)
struct GameAlarmPlayIntent: LiveActivityIntent {
    static var title: LocalizedStringResource = "Join"
    static var openAppWhenRun: Bool = true

    @Parameter(title: "Alarm ID")
    var alarmID: String?

    @Parameter(title: "Product ID")
    var productId: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        if let productId {
            let diagnostics = GameReminderDiagnostics(recorder: IssueMonitoringFactory.createHostEventRecorder())
            let startsAt = { storedStartsAt(productId: productId) }
            diagnostics.record(
                GameReminderEvent(
                    step: .tapped,
                    productId: productId,
                    startsAt: startsAt(),
                    appState: .init(UIApplication.shared.applicationState)
                )
            )

            if ProductOpener().open(productId: productId) {
                diagnostics.record(
                    GameReminderEvent(
                        step: .productOpened,
                        productId: productId,
                        startsAt: startsAt(),
                        openSource: .alarm,
                        appState: .init(UIApplication.shared.applicationState)
                    )
                )
            }
        }

        if let alarmIDString = alarmID,
           let alarmUUID = UUID(uuidString: alarmIDString) {
            try? AlarmManager.shared.stop(id: alarmUUID)
        }

        return .result()
    }

    /// The fire date the alarm was booked with, read from the settings that booking persisted.
    private func storedStartsAt(productId: String) -> Date? {
        SettingsManager.shared.integer(for: GameReminderStorageKeys.product(productId).alarmFireDate)
            .map { Date(timeIntervalSinceReferenceDate: TimeInterval($0)) }
    }
}
