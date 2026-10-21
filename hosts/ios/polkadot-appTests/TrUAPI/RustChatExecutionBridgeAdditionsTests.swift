import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The six callbacks of the Chat Extension v2 additions, through the rust chat bridge and the chat store
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

        let readAloudOnly = ChatRoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            readAloud: true,
            sealedAttachments: false,
            hopAttachments: false
        )
        try await bridge.setRoomFeatures(roomId: "lobby", features: readAloudOnly)
        #expect(try await bridge.roomFeatures(roomId: "lobby") == readAloudOnly)
    }

    /// S51: the host keeps a feature that it does not serve, so a later build applies it.
    @Test func aFeatureThatThisBuildDoesNotServeIsKept() async throws {
        let bridge = try await openRoom()
        #expect(!ChatHostCapabilities.served.contains("preimage_attachments"))

        try await bridge.setRoomFeatures(roomId: "lobby", features: everyFeature)

        let reread = await fixture.bridge()
        #expect(try await reread.roomFeatures(roomId: "lobby").readAloud)
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

        let session = Data(repeating: 9, count: 32)
        try await bridge.setRoomCall(
            roomId: "lobby",
            call: .active(callId: "c1", video: false, startedAt: 42, mediaSession: session)
        )
        #expect(fixture.calls.call(for: fixture.chatId("lobby")) == .active(
            callId: "c1",
            video: false,
            startedAt: 42,
            mediaSession: session
        ))

        try await bridge.setRoomCall(roomId: "lobby", call: nil)
        #expect(fixture.calls.call(for: fixture.chatId("lobby")) == nil)
    }

    @Test func theCallOfAnUnknownRoomIsRefused() async throws {
        let bridge = await fixture.bridge()

        await #expect(throws: (any Error).self) {
            try await bridge.setRoomCall(roomId: "nowhere", call: .ringing(callId: "c1", video: false, caller: nil))
        }
    }

    // MARK: - roomUserStates

    @Test func theUserStatesAreTheRoomsThatTheUserMutedOrArchived() async throws {
        let bridge = try await openRoom()
        for roomId in ["muted", "archived", "plain"] {
            _ = try await fixture.context.createRoom(for: fixture.bot, roomId: roomId, name: roomId, icon: nil)
        }
        try await saveUserState(.init(mutedUntil: Chat.RoomUserState.mutedForever, archived: false), roomId: "muted")
        try await saveUserState(.init(mutedUntil: nil, archived: true), roomId: "archived")

        let states = try await bridge.roomUserStates()

        #expect(states == [
            ChatRoomUserState(roomId: "archived", mutedUntil: nil, archived: true),
            ChatRoomUserState(roomId: "muted", mutedUntil: UInt64.max, archived: false)
        ])
    }

    @Test func aProductWhoseUserChangedNothingHasNoUserStates() async throws {
        let bridge = try await openRoom()

        #expect(try await bridge.roomUserStates().isEmpty)
    }

    // MARK: - bots

    @Test func theBotsAreThePersonasOfThisProductOnly() async throws {
        let bridge = await fixture.bridge()
        try await savePersona(productId: fixture.productId, botId: "helper")
        try await savePersona(productId: fixture.productId, botId: "critic")
        try await savePersona(productId: "other.dot", botId: "stranger")

        #expect(try await bridge.bots() == ["critic", "helper"])
    }

    @Test func aProductWithNoPersonaHasNoBots() async throws {
        let bridge = await fixture.bridge()

        #expect(try await bridge.bots().isEmpty)
    }

    // MARK: - registerBot

    /// S53: `New` the first time, then `Exists`, and a repeat replaces the name and the icon.
    @Test func aRepeatedRegistrationReplacesThePersona() async throws {
        let bridge = await fixture.bridge()
        let store = ChatProductPersonaStore(storageFacade: fixture.storageFacade)

        #expect(try await bridge.registerBot(botId: "helper", name: "Helper", icon: "") == .new)
        #expect(try await bridge.registerBot(botId: "helper", name: "Helper Two", icon: "data:image/png;base64,AA==")
            == .exists)

        let replaced = Chat.ProductPersona(
            productId: fixture.productId,
            botId: "helper",
            name: "Helper Two",
            icon: "data:image/png;base64,AA=="
        )
        #expect(try await store.personas(of: fixture.productId) == [replaced])
        #expect(try await bridge.bots() == ["helper"])
    }

    @Test func aNinthPersonaIsRefusedAndARepeatOfAKnownOneIsNot() async throws {
        let bridge = await fixture.bridge()
        for index in 0 ..< ChatProductPersonaStore.maxPerProduct {
            _ = try await bridge.registerBot(botId: "b\(index)", name: "Bot \(index)", icon: "")
        }

        await #expect(throws: HostRejection.self) {
            try await bridge.registerBot(botId: "b8", name: "Bot 8", icon: "")
        }
        #expect(try await bridge.registerBot(botId: "b0", name: "Bot zero", icon: "") == .exists)
        #expect(try await bridge.bots().count == ChatProductPersonaStore.maxPerProduct)
    }

    @Test func thePersonasOfOneProductDoNotCountForAnother() async throws {
        let bridge = await fixture.bridge()
        let store = ChatProductPersonaStore(storageFacade: fixture.storageFacade)
        for index in 0 ..< ChatProductPersonaStore.maxPerProduct {
            _ = try await store.register(Chat.ProductPersona(
                productId: "other.dot",
                botId: "b\(index)",
                name: "Other",
                icon: nil
            ))
        }

        #expect(try await bridge.registerBot(botId: "helper", name: "Helper", icon: "") == .new)
    }
}

private extension RustChatExecutionBridgeAdditionsTests {
    var everyFeature: ChatRoomFeatures {
        ChatRoomFeatures(
            preimageAttachments: true,
            search: true,
            calls: ChatRoomCalls(audio: true, video: false),
            readAloud: true,
            sealedAttachments: true,
            hopAttachments: true
        )
    }

    func openRoom() async throws -> RustChatExecutionBridge {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
        return await fixture.bridge()
    }

    func saveUserState(_ state: Chat.RoomUserState, roomId: String) async throws {
        try await fixture.context.saveDeclaration(Chat.RoomUserStateUpdate(
            chatId: fixture.chatId(roomId),
            state: state
        ))
    }

    func savePersona(productId: String, botId: String) async throws {
        try await fixture.storageFacade.databaseService.performWrite { context in
            let persona = try context.insertNew(CDProductChatPersona.self)
            persona.identifier = "\(productId)/\(botId)"
            persona.productId = productId
            persona.botId = botId
            persona.name = botId
        }
    }
}
