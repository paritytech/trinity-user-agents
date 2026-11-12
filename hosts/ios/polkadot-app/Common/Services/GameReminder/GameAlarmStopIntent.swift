import struct TrUAPIHost.ProductId
import Foundation
import AlarmKit
import AppIntents
import IssueMonitoring
import Keystore_iOS

@available(iOS 26.1, *)
struct GameAlarmPlayIntent: LiveActivityIntent {
    static var title: LocalizedStringResource = "Join"
    static var openAppWhenRun: Bool = true

    @Parameter(title: "Alarm ID")
    var alarmID: String?

    // AppIntents persists this parameter name in scheduled alarms.
    @Parameter(title: "Product ID")
    var productId: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        if let input = productId {
            let productId = try ProductId(input)
            let diagnostics = GameReminderDiagnostics(recorder: IssueMonitoringFactory.createHostEventRecorder())
            diagnostics.tapped(productId: productId, startsAt: storedStartsAt(productId: productId))

            if ProductOpener().open(productId: productId) {
                diagnostics.productOpened(
                    productId: productId,
                    startsAt: storedStartsAt(productId: productId),
                    source: .alarm
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
    private func storedStartsAt(productId: ProductId) -> Date? {
        SettingsManager.shared.integer(for: GameReminderStorageKeys.product(productId).alarmFireDate)
            .map { Date(timeIntervalSinceReferenceDate: TimeInterval($0)) }
    }
}
