import Foundation
import Testing
@testable import polkadot_app

@Suite("Alarm string resolution")
struct AlarmStringResolutionTests {
    /// An app update moves the bundle, so the system resolves an alarm scheduled earlier against a
    /// bundle path that no longer exists.
    private func resolvedAfterBundleMoved(_ resource: LocalizedStringResource) throws -> String {
        var json = try #require(
            try JSONSerialization.jsonObject(with: JSONEncoder().encode(resource)) as? [String: Any]
        )
        json["bundleURL"] = "file:///private/var/containers/Bundle/Application/REMOVED/polkadot-app.app/"
        let moved = try JSONDecoder().decode(
            LocalizedStringResource.self,
            from: JSONSerialization.data(withJSONObject: json)
        )
        return String(localized: moved)
    }

    @Test("A key resolved late shows the key once the bundle moved")
    func keyShowsAfterBundleMoved() throws {
        #expect(try resolvedAfterBundleMoved(.Notification.gameAlarmPlayGame) == "game.alarm.playGame")
    }

    @Test("Resolved text survives the bundle moving")
    func resolvedTextSurvivesBundleMove() throws {
        let resource = LocalizedStringResource.Notification.gameAlarmPlayGame
        #expect(try resolvedAfterBundleMoved(resource.resolvedText) == String(localized: resource))
    }
}
