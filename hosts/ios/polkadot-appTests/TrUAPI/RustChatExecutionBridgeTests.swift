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

    /// A room the product drives through actions alone asks for an empty footer, can ask for the text input
    /// back, and can draw the footer itself (P6.6).
    @Test func setRoomFooterForwardsTheFooter() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)
        try await bridge.setRoomFooter(roomId: "r", footer: .empty)
        let input = ChatInputConfig(placeholder: nil, photo: true, video: true, file: false, payment: nil)
        try await bridge.setRoomFooter(roomId: "r", footer: .input(input))
        try await bridge.setRoomFooter(roomId: "r", footer: .custom)

        #expect(api.footerConfigs.map(\.footer) == [.empty, .input(.hostDefault), .custom])
        #expect(api.footerConfigs.map(\.roomId) == ["r", "r", "r"])
    }

    @Test func setRoomFooterRejectsAnEmptyRoom() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        await #expect(throws: HostRejection.self) {
            try await bridge.setRoomFooter(roomId: "", footer: .empty)
        }
        #expect(api.footerConfigs.isEmpty)
    }

    @Test func postMessageForwardsTextAndCustom() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        let messageId = try await bridge.postMessage(roomId: "r", content: .text(text: "hi"), alt: nil)
        #expect(api.sentMessages.count == 1)
        #expect(api.sentRoomIds == ["r"])
        #expect(messageId == "msg-1")

        _ = try await bridge.postMessage(
            roomId: "r",
            content: .custom(ChatCustomMessage(messageType: "t", payload: Data([1]))),
            alt: "Week 12 results"
        )
        #expect(api.sentMessages.count == 2)
        if case let .custom(messageType, data, alt) = api.sentMessages.last {
            #expect(messageType == "t")
            #expect(data == Data([1]))
            #expect(alt == "Week 12 results")
        } else {
            Issue.record("a custom message must keep its type, payload and alt")
        }

        await #expect(throws: HostRejection.self) {
            try await bridge.postMessage(
                roomId: "r",
                content: .reaction(ChatReaction(messageId: "m", emoji: "x")),
                alt: nil
            )
        }
        #expect(api.sentMessages.count == 2)
    }

    @Test func postMessageForwardsRichTextWithItsMedia() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)
        let media = ["https://example.com/a.png", "data:image/png;base64,AAAA"]

        _ = try await bridge.postMessage(
            roomId: "r",
            content: .richText(ChatRichText(text: "**hi**", media: media.map { ChatMedia(url: $0) })),
            alt: nil
        )

        #expect(api.sentMessages == [.richText(text: "**hi**", media: media)])
    }

    @Test func postMessageForwardsActionsWithTheirLayout() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        _ = try await bridge.postMessage(
            roomId: "r",
            content: .actions(ChatActions(
                text: "Pick one",
                actions: [ChatAction(actionId: "yes", title: "Yes"), ChatAction(actionId: "no", title: "No")],
                layout: .grid
            )),
            alt: nil
        )

        let expected = ProductBotActions(
            text: "Pick one",
            buttons: [.init(actionId: "yes", title: "Yes"), .init(actionId: "no", title: "No")],
            layout: .grid
        )
        #expect(api.sentMessages == [.actions(expected)])
    }

    @Test func postMessageForwardsAFile() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        _ = try await bridge.postMessage(
            roomId: "r",
            content: .file(ChatFile(
                url: "https://example.com/report.pdf",
                fileName: "report.pdf",
                mimeType: "application/pdf",
                sizeBytes: 2_048,
                text: "Your report"
            )),
            alt: nil
        )

        let expected = ProductBotFile(
            url: "https://example.com/report.pdf",
            fileName: "report.pdf",
            mimeType: "application/pdf",
            sizeBytes: 2_048,
            text: "Your report"
        )
        #expect(api.sentMessages == [.file(expected)])
    }

    @Test func postMessageRejectsAReactionRemoval() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        await #expect(throws: HostRejection.self) {
            try await bridge.postMessage(
                roomId: "r",
                content: .reactionRemoved(ChatReaction(messageId: "m", emoji: "x")),
                alt: nil
            )
        }
        #expect(api.sentMessages.isEmpty)
    }

    /// A failing surface must surface as a rejection, not a silent success.
    @Test func postMessageRejectsWhenTheSurfaceFails() async throws {
        let api = RecordingChatMessaging()
        api.sendMessageError = ProductNativeApiError.messagesNotSupported
        let bridge = await makeBridge(api: api)

        await #expect(throws: ProductNativeApiError.self) {
            try await bridge.postMessage(roomId: "r", content: .text(text: "hi"), alt: nil)
        }
    }

    /// A roomless body cannot be named by a `RenderContext`, so nothing could
    /// ever draw it: refuse the post rather than file it under the bot's
    /// default chat.
    @Test func postMessageRejectsAnEmptyRoom() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        await #expect(throws: HostRejection.self) {
            try await bridge.postMessage(roomId: "", content: .text(text: "hi"), alt: nil)
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

    /// P10.6: `register_bot` keeps a persona. An empty icon means no icon.
    @Test func registerBotKeepsAPersona() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        #expect(try await bridge.registerBot(botId: "b", name: "n", icon: "") == .new)
        #expect(try await bridge.registerBot(botId: "b", name: "m", icon: "https://e.com/i.png") == .exists)
        #expect(api.registeredPersonas == [
            .init(botId: "b", name: "n", icon: nil),
            .init(botId: "b", name: "m", icon: "https://e.com/i.png")
        ])
    }

    @Test func registerBotRejectsAnEmptyId() async throws {
        let api = RecordingChatMessaging()
        let bridge = await makeBridge(api: api)

        await #expect(throws: HostRejection.self) {
            try await bridge.registerBot(botId: "", name: "n", icon: "")
        }
        #expect(api.registeredPersonas.isEmpty)
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
