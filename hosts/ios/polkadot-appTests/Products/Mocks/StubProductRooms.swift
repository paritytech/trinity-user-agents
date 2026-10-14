import Foundation
@testable import polkadot_app

/// Answers every room with the same declarations, and the newest message by the order of a given list.
struct StubProductRooms: ProductRoomReading {
    var declarations: Chat.RoomDeclarations?
    /// The stored ids of the room, oldest first.
    var messageIds: [Chat.MessageId] = []

    func declarations(of _: Chat.Id) async -> Chat.RoomDeclarations? {
        declarations
    }

    func newestMessageId(among ids: Set<Chat.MessageId>, in _: Chat.Id) async -> Chat.MessageId? {
        messageIds.last { ids.contains($0) }
    }
}
