import AlarmKit
import AppIntents
import UIKit

@available(iOS 26.1, *)
struct GameAlarmPlayIntent: LiveActivityIntent {
    static var title: LocalizedStringResource = "Join"
    static var openAppWhenRun: Bool = true

    @Parameter(title: "Alarm ID")
    var alarmID: String?

    @Parameter(title: "Game Index")
    var gameIndex: Int?

    @Parameter(title: "Product ID")
    var productId: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        if let productId {
            ProductOpener().open(productId: productId)
        } else {
            await UIApplication.shared.open(AppConfig.DeepLink.game(intendedGameIndex: gameIndex))
        }

        if let alarmIDString = alarmID,
           let alarmUUID = UUID(uuidString: alarmIDString) {
            try? AlarmManager.shared.stop(id: alarmUUID)
        }

        return .result()
    }
}
