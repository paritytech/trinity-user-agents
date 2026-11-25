import Foundation
@testable import polkadot_app

final class StubPushNotificationsCleaner: PushNotificationsCleaning {
    func cleanNotifications(for _: Chat.Id, messageIds _: [Chat.MessageId]) async throws {}
}
