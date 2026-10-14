import Foundation
import AsyncExtensions
import Products
@testable import polkadot_app

/// Records the chat calls `RustChatExecutionBridge` makes. Conforms to the
/// narrow port rather than the whole native api, so no binding is needed.
final class RecordingChatMessaging: ProductChatMessaging, @unchecked Sendable {
    var createRoomStatus: CreateRoomStatus = .new
    var roomsToReturn: [RoomInfo] = []
    var sendMessageError: Error?

    private(set) var sentMessages: [ProductBotMessage] = []
    private(set) var sentRoomIds: [String?] = []
    private(set) var createdRooms: [CreateRoomRequest] = []
    private(set) var roomFooters: [(roomId: String, footer: Chat.RoomFooter)] = []
    private(set) var subscribeRoomsCallCount = 0

    func sendMessage(_ message: ProductBotMessage, roomId: String?) async throws -> String {
        if let sendMessageError { throw sendMessageError }
        sentMessages.append(message)
        sentRoomIds.append(roomId)
        return "msg-\(sentMessages.count)"
    }

    func createRoom(_ request: CreateRoomRequest) async throws -> CreateRoomResult {
        createdRooms.append(request)
        return CreateRoomResult(status: createRoomStatus)
    }

    func setRoomFooter(roomId: String, footer: Chat.RoomFooter) async throws {
        roomFooters.append((roomId, footer))
    }

    func subscribeRooms() async throws -> AnyAsyncSequence<[RoomInfo]> {
        subscribeRoomsCallCount += 1
        let rooms = roomsToReturn
        return AsyncStream<[RoomInfo]> { continuation in
            continuation.yield(rooms)
            continuation.finish()
        }.eraseToAnyAsyncSequence()
    }
}
