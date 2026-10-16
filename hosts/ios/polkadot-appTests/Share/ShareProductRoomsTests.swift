import Foundation
import Testing
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// The share sheet offers the rooms of a product that keep their messages in the product, and a shared link goes to
/// such a room as a message of the user (PARITY row D17).
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

    @Test func theSharedTextIsANewMessageOfTheUserInTheRoom() async throws {
        let rooms = ShareProductRooms(registry: SingleBotRegistry(bot: QueryingBot(querying: querying)))

        try await rooms.send(text: "https://chat.dot.li", to: .chatExtension("game.dot", roomId: "dm-7"))

        #expect(querying.actionRequests == [ProductChatUserActionRequest(
            roomId: "dm-7",
            action: .newMessage(body: .text(text: "https://chat.dot.li"), replyTo: nil)
        )])
    }

    @Test func aRefusalOfTheProductReachesTheShareSheet() async {
        let refusing = ScriptedProductChatQuerying(action: { _ in
            throw ChatQueryError.actionRefused(.rejected(reason: "not connected"))
        })
        let rooms = ShareProductRooms(registry: SingleBotRegistry(bot: QueryingBot(querying: refusing)))

        await #expect(throws: ShareProductRoomError.refused(reason: "not connected")) {
            try await rooms.send(text: "x", to: .chatExtension("game.dot", roomId: "dm-7"))
        }
        await #expect(throws: ShareProductRoomError.productNotRunning) {
            try await rooms.send(text: "x", to: .chatExtension("other.dot", roomId: "dm-7"))
        }
    }

    private func room(productSource: Bool, product: String = "game.dot") -> Chat.LocalModel {
        Chat.LocalModel(
            peer: .chatExtension(product, roomId: "dm-7"),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil,
            roomDeclarations: Chat.RoomDeclarations(
                description: nil,
                avatar: nil,
                config: Chat.RoomConfig(
                    productSource: productSource,
                    reactionDetails: false,
                    participants: false,
                    messageActions: false,
                    participantActions: false,
                    allowedReactions: nil,
                    placeholder: nil,
                    reportSeen: false
                ),
                configGeneration: 0,
                preview: .hostDefault,
                toolbar: nil,
                suggestions: nil,
                commands: nil,
                footerConfig: nil
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
