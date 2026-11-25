import Foundation
import AsyncExtensions
import Products
@testable import polkadot_app

/// Records the chat calls `RustChatExecutionBridge` makes. Conforms to the
/// narrow port rather than the whole native api, so no binding is needed.
final class RecordingChatMessaging: ProductChatMessaging, @unchecked Sendable {
    var createRoomStatus: CreateRoomStatus = .new
    var roomsToReturn: [RoomInfo] = []
    /// The room stream ends before it yields a list, as a cancelled subscription does.
    var endsRoomListWithoutRooms = false
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
        let endsWithoutRooms = endsRoomListWithoutRooms
        return AsyncStream<[RoomInfo]> { continuation in
            if !endsWithoutRooms {
                continuation.yield(rooms)
            }
            continuation.finish()
        }.eraseToAnyAsyncSequence()
    }
}
