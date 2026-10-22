import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The callbacks of the Chat Extension v2 additions, through the rust chat bridge and the chat store
/// (07-LATER §4.4, P10.8.7).
struct RustChatExecutionBridgeAdditionsTests {
    private let fixture = ProductRoomFixture()

    // MARK: - capabilities

    @Test func theCapabilitiesAreTheNamesThatThisBuildServes() async throws {
        let bridge = await fixture.bridge()

        #expect(try await bridge.capabilities() == ChatHostCapabilities.served)
    }

    // MARK: - roomFeatures, setRoomFeatures

    @Test func aRoomWithNoDeclaredFeaturesAnswersTheHostDefault() async throws {
        let bridge = try await openRoom()

        #expect(try await bridge.roomFeatures(roomId: "lobby") == .hostDefault)
    }

    @Test func theFeaturesOfARoomPersistWholeAndReplaceTheEarlierOnes() async throws {
        let bridge = try await openRoom()

        try await bridge.setRoomFeatures(roomId: "lobby", features: everyFeature)
        #expect(try await bridge.roomFeatures(roomId: "lobby") == everyFeature)
        #expect(try await fixture.storedChat("lobby")?.roomPolicy?.features == Chat.RoomFeatures(everyFeature))

        let noCalls = ChatRoomFeatures(calls: nil, hopAttachments: false)
        try await bridge.setRoomFeatures(roomId: "lobby", features: noCalls)
        #expect(try await bridge.roomFeatures(roomId: "lobby") == noCalls)
    }

    /// S51: the features of a room survive a restart of the bridge.
    @Test func theFeaturesOfARoomSurviveARestart() async throws {
        let bridge = try await openRoom()

        try await bridge.setRoomFeatures(roomId: "lobby", features: everyFeature)

        let reread = await fixture.bridge()
        #expect(try await reread.roomFeatures(roomId: "lobby") == everyFeature)
    }

    @Test func theFeaturesOfAnUnknownRoomAreRefused() async throws {
        let bridge = await fixture.bridge()

        await #expect(throws: (any Error).self) {
            try await bridge.setRoomFeatures(roomId: "nowhere", features: everyFeature)
        }
        await #expect(throws: (any Error).self) {
            _ = try await bridge.roomFeatures(roomId: "nowhere")
        }
    }

    // MARK: - setRoomCall

    @Test func theCallOfARoomStaysInMemoryAndARemovalClearsIt() async throws {
        let bridge = try await openRoom()
        let ringing = ChatRoomCall.ringing(callId: "c1", video: true, caller: "p1")

        try await bridge.setRoomCall(roomId: "lobby", call: ringing)
        #expect(fixture.calls.call(for: fixture.chatId("lobby")) == .ringing(callId: "c1", video: true, caller: "p1"))

        try await bridge.setRoomCall(roomId: "lobby", call: .active(callId: "c1", video: false, startedAt: 42))
        #expect(fixture.calls.call(for: fixture.chatId("lobby")) == .active(callId: "c1", video: false, startedAt: 42))

        try await bridge.setRoomCall(roomId: "lobby", call: nil)
        #expect(fixture.calls.call(for: fixture.chatId("lobby")) == nil)
    }

    @Test func theCallOfAnUnknownRoomIsRefused() async throws {
        let bridge = await fixture.bridge()

        await #expect(throws: (any Error).self) {
            try await bridge.setRoomCall(roomId: "nowhere", call: .ringing(callId: "c1", video: false, caller: nil))
        }
    }
}

private extension RustChatExecutionBridgeAdditionsTests {
    var everyFeature: ChatRoomFeatures {
        ChatRoomFeatures(calls: ChatRoomCalls(audio: true, video: false), hopAttachments: true)
    }

    func openRoom() async throws -> RustChatExecutionBridge {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
        return await fixture.bridge()
    }
}
