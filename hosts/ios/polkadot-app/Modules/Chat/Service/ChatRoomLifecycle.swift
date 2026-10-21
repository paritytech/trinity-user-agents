import Foundation

/// Opens and closes the room of a product while the room screen shows it (02-DESIGN §5.2, §8.4).
///
/// `open` holds the worker of the product and sends `RoomOpened`. `close` sends `RoomClosed` and releases
/// the worker. Each `open` that does not throw gets one `close`.
protocol ChatRoomLifecycle: Sendable {
    /// Throws when the worker of the product cannot start. Then the room holds nothing.
    func open(roomId: String) async throws
    func close(roomId: String) async
}

/// The lifecycle of a room whose product has no lifecycle: a person chat, or a host without P5.4.
struct NoChatRoomLifecycle: ChatRoomLifecycle {
    func open(roomId _: String) async throws {}
    func close(roomId _: String) async {}
}
