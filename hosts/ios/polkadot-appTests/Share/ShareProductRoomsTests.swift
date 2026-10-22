import Foundation
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// The share sheet offers the rooms of the chat product with a contact that both sides accepted, as the native chat
/// offers its person chats, and a shared link goes to such a room as a message of the user (PARITY row D17).
struct ShareProductRoomsTests {
    private let querying = ScriptedProductChatQuerying()

    @Test func aRoomThatTheProductKeepsIsATarget() {
        let rooms = ShareProductRooms(registry: SingleBotRegistry(bot: QueryingBot(querying: querying)))

        #expect(rooms.target(for: room(productSource: true))?.chat.chatId
            == .chatExtension("game.dot", roomId: "dm-7"))
    }

    @Test func aRoomThatTheHostStoresOrOfAnotherProductIsNoTarget() {
        let rooms = ShareProductRooms(registry: SingleBotRegistry(bot: QueryingBot(querying: querying)))

        #expect(rooms.target(for: room(productSource: false)) == nil)
        #expect(rooms.target(for: room(productSource: true, product: "other.dot")) == nil)
    }

    @Test func aRoomWithoutAnAcceptedContactIsNoTarget() {
        let rooms = ShareProductRooms(registry: SingleBotRegistry(bot: QueryingBot(querying: querying)))

        #expect(rooms.target(for: room(productSource: true, contact: nil)) == nil)
        #expect(rooms.target(for: room(productSource: true, contact: .outgoing)) == nil)
        #expect(rooms.target(for: room(productSource: true, contact: .blocked)) == nil)
        #expect(rooms.target(for: room(productSource: true, contact: .peerLeft)) != nil)
    }

    @Test func theSharedTextIsANewMessageOfTheUserInTheRoom() async throws {
        let outbox = ProductOutbox.inMemory(querying: querying)
        let rooms = ShareProductRooms(registry: SingleBotRegistry(bot: QueryingBot(querying: querying)), outbox: outbox)

        try await rooms.send(text: "https://chat.dot.li", to: .chatExtension("game.dot", roomId: "dm-7"))
        await outbox.waitUntilIdle(productId: "game.dot", roomId: "dm-7")

        let request = try #require(querying.actionRequests.first)
        guard case let .newMessage(body, replyTo, messageId, _) = request.action else {
            Issue.record("not a new message: \(request.action)")
            return
        }
        #expect(request.roomId == "dm-7")
        #expect(body == .text(text: "https://chat.dot.li"))
        #expect(replyTo == nil)
        // The host chooses the id of the message, a UUID as the native chat names its messages.
        #expect(messageId.flatMap(UUID.init(uuidString:)) != nil)
    }

    @Test func aChatOfNoProductRoomTakesNoSharedText() async {
        let rooms = ShareProductRooms(
            registry: SingleBotRegistry(bot: QueryingBot(querying: querying)),
            outbox: .inMemory(querying: querying)
        )

        await #expect(throws: ShareProductRoomError.noRoom) {
            try await rooms.send(text: "x", to: .chatExtension("game.dot", roomId: nil))
        }
    }

    private func room(
        productSource: Bool,
        product: String = "game.dot",
        contact: Chat.RoomContact.State? = .established
    ) -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension(product, roomId: "dm-7"),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil,
            roomDeclarations: Chat.RoomDeclarations(
                config: Chat.RoomConfig(productSource: productSource, reactionDetails: false, reportSeen: false),
                configGeneration: 0,
                preview: .hostDefault,
                contact: contact.map { state in
                    Chat.RoomContact(account: Data(repeating: 1, count: 32), username: "alice", state: state)
                }
            )
        )
    }
}

/// A product bot that answers the user actions of its rooms through `querying`.
private final class QueryingBot: ChatExtensionBotProtocol, ProductChatQuerying, @unchecked Sendable {
    let identifier: ChatExtension.Id = "game.dot"
    private let querying: ScriptedProductChatQuerying

    init(querying: ScriptedProductChatQuerying) {
        self.querying = querying
    }

    var peerMetadata: Chat.PeerMetadata {
        Chat.PeerMetadata(name: "Game", icon: .product(domain: identifier), input: .empty, moreActions: [])
    }

    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
        try await querying.messagesPage(request)
    }

    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
        try await querying.userAction(request)
    }

    func deliverAutomaticMessages(_: ChatExtensionDiscoverContextProtocol) {}

    func attach(presentationView _: ControllerBackedProtocol) {}

    func process(
        message _: Chat.LocalMessage,
        lastProcessingOutcome _: ChatExtension.ProcessingHistoryOutcome,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        .skipped
    }

    func process(action _: Chat.Action, context _: ChatExtensionActionContextProtocol) async {}
}
