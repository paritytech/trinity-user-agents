import Foundation
import os
import PolkadotUI
import Products
import Testing
import UIKitExt
@testable import polkadot_app

/// What the user does in a room that the host stores reaches the product as a Chat Extension v2 event
/// (P4.3, scenarios S12 and S14).
struct ProductBotChatEventsTests {
    private let runtime = RecordingEventsRuntime()
    private let chatId = Chat.Id.chatExtension("game.dot", roomId: "lobby")

    @Test func aPostedTextIsMessagePostedWithTheIdOfTheHost() async {
        let bot = makeBot()

        let result = await process(bot, userMessage(.text("hello"), id: "u1"))

        #expect(result == .processed)
        #expect(runtime.events == [.init(
            event: .messagePosted(messageId: "u1", text: "hello", replyTo: nil),
            roomId: "lobby"
        )])
    }

    /// The product knows a message that it pushed by its own id, so a reply names that id.
    @Test func aReplyNamesTheRepliedMessageByTheIdOfTheProduct() async {
        let bot = makeBot()
        let pushed = Chat.ProductMessageId.stored("m1", in: chatId)
        let reply = Chat.RemoteMessageContentV1.MessageContent.ReplyContent(
            messageId: pushed,
            ownContent: .init(text: "yes", attachments: nil)
        )

        await process(bot, userMessage(.reply(reply), id: "u2"))

        #expect(runtime.events.map(\.event) == [.messagePosted(messageId: "u2", text: "yes", replyTo: "m1")])
    }

    @Test func anEditIsUserEdited() async {
        let bot = makeBot()
        let edit = Chat.RemoteMessageContentV1.MessageContent.EditedContent(
            messageId: "u1",
            newContent: .init(text: "hello again", attachments: nil)
        )

        await process(bot, userMessage(.edited(edit)))

        #expect(runtime.events.map(\.event) == [.edited(messageId: "u1", text: "hello again")])
    }

    @Test func aReactionIsUserReactedWhenPlacedAndWhenRemoved() async {
        let bot = makeBot()
        let pushed = Chat.ProductMessageId.stored("m1", in: chatId)

        await process(bot, userMessage(.reacted(.init(messageId: pushed, emoji: "👍"))))
        await process(bot, userMessage(.reactionRemoved(.init(messageId: pushed, emoji: "👍"))))

        #expect(runtime.events.map(\.event) == [
            .reacted(messageId: "m1", emoji: "👍", placed: true),
            .reacted(messageId: "m1", emoji: "👍", placed: false)
        ])
    }

    @Test func anActionResponseIsAPostedMessage() async {
        let bot = makeBot()

        await process(bot, userMessage(.extensionActionResponse("Yes", "yes"), id: "u3"))

        #expect(runtime.events.map(\.event) == [.messagePosted(messageId: "u3", text: "Yes", replyTo: nil)])
    }

    @Test func aMessageOfTheProductOrOneProcessedBeforeIsNoEvent() async {
        let bot = makeBot()
        let productMessage = Chat.LocalMessage.newExtensionMessage("game.dot", roomId: "lobby", content: .text("hi"))

        #expect(await process(bot, productMessage) == .skipped)
        #expect(await process(bot, userMessage(.text("old")), outcome: .previouslyProcessed) == .skipped)
        #expect(runtime.events.isEmpty)
    }

    @Test func aDeleteIsUserDeletedWithTheIdOfTheProduct() async {
        let bot = makeBot()

        await bot.userDeleted(messageId: Chat.ProductMessageId.stored("m1", in: chatId), roomId: "lobby")
        await bot.userDeleted(messageId: "u1", roomId: "lobby")

        #expect(runtime.events.map(\.event) == [.deleted(messageId: "m1"), .deleted(messageId: "u1")])
    }

    /// `report_seen` asks for `Seen` with the newest message that the user saw.
    @Test func seenNamesTheNewestMessageWhenTheRoomAsks() async {
        let pushed = Chat.ProductMessageId.stored("m2", in: chatId)
        let bot = makeBot(config: config(reportSeen: true), messageIds: ["m0", "u1", pushed, "m3"])

        await bot.userSaw(messageIds: ["u1", pushed], roomId: "lobby")

        #expect(runtime.events.map(\.event) == [.seen(messageId: "m2")])
    }

    @Test func seenIsNoEventWhenTheRoomDoesNotAsk() async {
        let bot = makeBot(config: config(reportSeen: false), messageIds: ["u1"])

        await bot.userSaw(messageIds: ["u1"], roomId: "lobby")
        await bot.userSaw(messageIds: ["u1"], roomId: nil)

        #expect(runtime.events.isEmpty)
    }

    /// 02-DESIGN §13 item 11: in a room whose product keeps the messages these actions go to `user_action`.
    @Test func aRoomWhoseProductKeepsTheMessagesGetsNoUserEvents() async {
        var productSource = config(reportSeen: true)
        productSource = Chat.RoomConfig(
            productSource: true,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            participantActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: productSource.reportSeen
        )
        let bot = makeBot(config: productSource, messageIds: ["u1"])

        await process(bot, userMessage(.text("hello")))
        await bot.userDeleted(messageId: "u1", roomId: "lobby")
        await bot.userSaw(messageIds: ["u1"], roomId: "lobby")

        #expect(runtime.events.isEmpty)
    }
}

private extension ProductBotChatEventsTests {
    func config(reportSeen: Bool) -> Chat.RoomConfig {
        Chat.RoomConfig(
            productSource: false,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            participantActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: reportSeen
        )
    }

    func makeBot(config: Chat.RoomConfig? = nil, messageIds: [Chat.MessageId] = []) -> ProductBot {
        let declarations = Chat.RoomDeclarations(
            description: nil,
            avatar: nil,
            config: config,
            configGeneration: 0,
            preview: .hostDefault,
            toolbar: nil,
            suggestions: nil,
            commands: nil,
            footerConfig: nil
        )
        return ProductBot(
            product: Product(id: "game.dot", name: "Game"),
            description: nil,
            runtime: runtime,
            resolveImage: WidgetImageResolver { _ in nil },
            rooms: StubProductRooms(declarations: declarations, messageIds: messageIds)
        )
    }

    func userMessage(_ content: Chat.LocalMessage.Content, id: String = UUID().uuidString) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: id,
            chatId: chatId,
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.delivered),
            timestamp: 1,
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    @discardableResult
    func process(
        _ bot: ProductBot,
        _ message: Chat.LocalMessage,
        outcome: ChatExtension.ProcessingHistoryOutcome = .firstEncounter
    ) async -> ChatExtension.ProcessingResult {
        await bot.process(message: message, lastProcessingOutcome: outcome, context: InertProcessingContext())
    }
}

private struct InertProcessingContext: ChatExtensionProcessingContextProtocol {
    func addNewMessage(
        _: Chat.LocalMessage.Content,
        delayDelivery _: MessageDeliveryDelay,
        chatExtension _: ChatExtending
    ) async throws {}

    func modifyMessageContent(messageId _: Chat.MessageId, content _: Chat.LocalMessage.Content) async throws {}
}

private final class RecordingEventsRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    struct Published: Equatable {
        let event: ProductChatUserEvent
        let roomId: String?
    }

    private let published = OSAllocatedUnfairLock(initialState: [Published]())

    var events: [Published] { published.withLock { $0 } }

    func publishUserEvent(_ event: ProductChatUserEvent, roomId: String?) async throws {
        published.withLock { $0.append(Published(event: event, roomId: roomId)) }
    }

    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {}
    func onUserMessage(text _: String, roomId _: String?) async throws {}

    func renderMessage(
        roomId _: String?,
        messageId _: String,
        messageType _: String,
        messageData _: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        AsyncThrowingStream { $0.finish() }
    }

    func dispatchEvent(
        roomId _: String?,
        messageId _: String,
        messageType _: String?,
        actionId _: String,
        payload _: String?
    ) async {}

    func dispatchActionTrigger(roomId _: String?, messageId _: String, actionId _: String) async {}
    func dispatchCommand(_: Chat.Command, roomId _: String?) async -> Bool { false }
    @MainActor func attach(presentationView _: ControllerBackedProtocol) {}
    func dispose() async {}
}
