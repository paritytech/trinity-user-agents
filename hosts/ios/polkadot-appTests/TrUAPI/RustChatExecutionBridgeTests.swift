import AsyncExtensions
import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

struct RustChatExecutionBridgeTests {
    private func makeBridge(api: any ProductChatMessaging) async -> RustChatExecutionBridge {
        await RustChatExecutionBridge(
            dependencies: MainActor.run { makeChatBridgeDependencies() },
            chatMessaging: api
        )
    }

    @Test func createRoomMapsRegistrationStatus() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        api.createRoomStatus = .new
        #expect(try await bridge.createRoom(roomId: "r", name: "n", icon: "i") == .new)

        api.createRoomStatus = .exists
        #expect(try await bridge.createRoom(roomId: "r", name: "n", icon: "i") == .exists)
    }

    /// Empty name and icon mean "unset" to the native api, not empty strings.
    @Test func createRoomNormalisesEmptyFields() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)
        _ = try await bridge.createRoom(roomId: "r", name: "", icon: "")
        _ = try await bridge.createRoom(roomId: "r2", name: "kept", icon: "icon")

        #expect(api.createdRooms.first?.name == nil)
        #expect(api.createdRooms.first?.icon == nil)
        #expect(api.createdRooms.last?.name == "kept")
        #expect(api.createdRooms.last?.icon == "icon")
    }

    @Test func postMessageForwardsTextAndCustomOnly() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        let messageId = try await bridge.postMessage(roomId: "r", content: .text(text: "hi"))
        #expect(api.sentMessages.count == 1)
        #expect(api.sentRoomIds == ["r"])
        #expect(messageId == "msg-1")

        _ = try await bridge.postMessage(
            roomId: "r",
            content: .custom(ChatCustomMessage(messageType: "t", payload: Data([1])))
        )
        #expect(api.sentMessages.count == 2)
        if case let .custom(messageType, data) = api.sentMessages.last {
            #expect(messageType == "t")
            #expect(data == Data([1]))
        } else {
            Issue.record("a custom message must keep its type and payload")
        }

        await #expect(throws: HostRejection.self) {
            try await bridge.postMessage(
                roomId: "r",
                content: .reaction(ChatReaction(messageId: "m", emoji: "x"))
            )
        }
        #expect(api.sentMessages.count == 2)
    }

    /// A failing surface must surface as a rejection, not a silent success.
    @Test func postMessageRejectsWhenTheSurfaceFails() async throws {
        let api = RecordingChatMessaging()
        api.sendMessageError = ProductNativeApiError.messagesNotSupported
        let bridge = await makeBridge(api: api)

        await #expect(throws: ProductNativeApiError.self) {
            try await bridge.postMessage(roomId: "r", content: .text(text: "hi"))
        }
    }

    /// A roomless body cannot be named by a `RenderContext`, so nothing could
    /// ever draw it: refuse the post rather than file it under the bot's
    /// default chat.
    @Test func postMessageRejectsAnEmptyRoom() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        await #expect(throws: HostRejection.self) {
            try await bridge.postMessage(roomId: "", content: .text(text: "hi"))
        }
        #expect(api.sentMessages.isEmpty)
    }

    /// The same validation `postMessage` applies: an empty id names no room.
    @Test func createRoomRejectsAnEmptyRoom() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        await #expect(throws: HostRejection.self) {
            try await bridge.createRoom(roomId: "", name: "n", icon: "")
        }
        #expect(api.createdRooms.isEmpty)
    }

    @Test func registerBotIsRejected() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        await #expect(throws: HostRejection.self) {
            try await bridge.registerBot(botId: "b", name: "n", icon: "i")
        }
    }

    @Test func listRoomsReadsTheCurrentRooms() async throws {
        let api = RecordingChatMessaging()
        api.roomsToReturn = [RoomInfo(roomId: "stored", name: nil, icon: nil, participation: .roomHost)]
        let bridge = await makeBridge(api: api)

        let rooms = try await bridge.listRooms()

        #expect(rooms.map(\.roomId) == ["stored"])
        #expect(rooms.map(\.participatingAs) == [.roomHost])
        #expect(api.subscribeRoomsCallCount == 1)
    }

    @Test func listRoomsReportsNoRoomsWhenTheApiHasNone() async throws {
        let api = RecordingChatMessaging()
        api.roomsToReturn = []
        let bridge = await makeBridge(api: api)

        #expect(try await bridge.listRooms().isEmpty)
    }
}
