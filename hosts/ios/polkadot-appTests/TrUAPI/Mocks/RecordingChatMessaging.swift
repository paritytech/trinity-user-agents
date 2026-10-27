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
    private(set) var roomPreviews: [(roomId: String, preview: Chat.RoomPreview)] = []
    struct RoomMetadataCall: Equatable {
        let roomId: String
        let name: String
    }

    private(set) var roomMetadata: [RoomMetadataCall] = []
    private(set) var deletedRoomIds: [String] = []

    /// Rooms that every room call refuses with `ChatRoomError.unknownRoom`.
    var unknownRooms: Set<String> = []
    var declarations = Chat.RoomDeclarations(entity: nil)
    private(set) var roomConfigs: [(roomId: String, config: Chat.RoomConfig)] = []

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

    func deleteRoom(roomId: String) async throws {
        try requireRoom(roomId)
        deletedRoomIds.append(roomId)
    }

    func setRoomPreview(roomId: String, preview: Chat.RoomPreview) async throws {
        try requireRoom(roomId)
        roomPreviews.append((roomId, preview))
    }

    func setRoomMetadata(roomId: String, name: String) async throws {
        try requireRoom(roomId)
        roomMetadata.append(RoomMetadataCall(roomId: roomId, name: name))
    }

    func roomDeclarations(roomId: String) async throws -> Chat.RoomDeclarations {
        try requireRoom(roomId)
        return declarations
    }

    func setRoomConfig(roomId: String, config: Chat.RoomConfig) async throws {
        try requireRoom(roomId)
        roomConfigs.append((roomId, config))
    }

    private func requireRoom(_ roomId: String) throws {
        if unknownRooms.contains(roomId) { throw ChatRoomError.unknownRoom(roomId) }
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
