import Foundation
import Testing
@testable import polkadot_app

/// The open room of the chat product is one contact: a notification of another contact still opens its room, as the
/// native chat opens another person chat.
struct ProductRoomVisibilityTests {
    @Test func onlyTheOpenRoomIsVisible() {
        let controller = PushForegroundPresentationController()
        controller.updateVisibleScreen(.chatExtension(extensionId: "chat.dot", roomId: "dm-01"))

        #expect(controller.isChatVisible(for: .chatExtension("chat.dot", roomId: "dm-01")))
        #expect(!controller.isChatVisible(for: .chatExtension("chat.dot", roomId: "dm-02")))
        #expect(!controller.isChatVisible(for: .chatExtension("other.dot", roomId: "dm-01")))
    }

    /// A chat extension notification names no room. Any open room of the extension shows it, as before rooms.
    @Test func aNotificationWithoutARoomIsVisibleInAnyRoomOfItsExtension() {
        let controller = PushForegroundPresentationController()
        controller.updateVisibleScreen(.chatExtension(extensionId: "game.dot", roomId: "lobby"))

        #expect(controller.isChatVisible(for: .chatExtension("game.dot", roomId: nil)))
        #expect(!controller.isChatVisible(for: .chatExtension("other.dot", roomId: nil)))
    }
}
