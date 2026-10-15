import Foundation
import TrUAPIHost

/// Sends room events through the shared ``TrUAPIHostRuntime``.
final class TrUAPIChatRoomEventPublisher: ChatRoomEventPublishing, @unchecked Sendable {
    private let runtimeProvider: TrUAPIHostRuntimeProviding

    init(runtimeProvider: TrUAPIHostRuntimeProviding) {
        self.runtimeProvider = runtimeProvider
    }

    func publish(_ event: Chat.RoomEvent, productId: ChatExtension.Id, roomId: String) throws {
        try runtimeProvider.sharedRuntime().publishChatRoomEvent(
            productId: productId,
            roomId: roomId,
            event: event.wireValue
        )
    }
}
