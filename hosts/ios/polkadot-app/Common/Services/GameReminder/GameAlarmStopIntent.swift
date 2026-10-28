import struct TrUAPIHost.ProductId
import Foundation
import AlarmKit
import AppIntents

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
            ProductOpener().open(productId: productId)
        }

        if let alarmIDString = alarmID,
           let alarmUUID = UUID(uuidString: alarmIDString) {
            try? AlarmManager.shared.stop(id: alarmUUID)
        }

        return .result()
    }
}
