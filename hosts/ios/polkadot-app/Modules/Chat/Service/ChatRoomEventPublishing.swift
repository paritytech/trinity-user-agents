import Foundation

/// Sends what the user does with the Chat Extension v2 additions of a room to the product of the room
/// (`room_events_subscribe`, 07-LATER §4.3). The core keeps the newest 64 events of a product until its worker
/// subscribes, so the worker does not have to run.
protocol ChatRoomEventPublishing: Sendable {
    func publish(_ event: Chat.RoomEvent, productId: ChatExtension.Id, roomId: String) throws
}
