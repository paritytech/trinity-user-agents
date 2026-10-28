import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
import TrUAPIHost
@testable import polkadot_app

/// `update_feed` into a room that the host stores, through the rust chat bridge and the chat store.
struct ChatProductFeedTests {
    private let fixture = ProductRoomFixture()

    // MARK: - New

    @Test func aNewTextIsAProductMessageUnderARoomScopedId() async throws {
        let bridge = try await openRoom()

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "hi"))),
            persist: true
        )

        let stored = try #require(try await fixture.messages("lobby").last)
        #expect(stored.messageId == Chat.ProductMessageId.stored("m1", in: fixture.chatId("lobby")))
        #expect(stored.origin == .chatExtension(fixture.productId))
        #expect(stored.content == .text("hi"))
        #expect(stored.status == .incoming(.new))
        #expect(stored.timestamp == 1_700_000_000_000)
        #expect(try await fixture.storedChat("lobby")?.message?.messageId == stored.messageId)
    }

    @Test(arguments: BodyCase.all)
    func eachBodyKindIsStoredAsAContentTheRoomDraws(_ bodyCase: BodyCase) async throws {
        let bridge = try await openRoom()

        try await bridge.updateFeed(roomId: "lobby", update: .new(message("m1", body: bodyCase.body)), persist: true)

        #expect(try await fixture.messages("lobby").last?.content == bodyCase.content)
    }

    @Test func aTextThatRepliesQuotesTheMessageOfTheRoom() async throws {
        let bridge = try await openRoom()
        let user = fixture.userMessage(.text("question"), roomId: "lobby")
        try await fixture.save(user)

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "answer"), replyTo: user.messageId)),
            persist: true
        )
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m2", body: .text(text: "again"), replyTo: "m1")),
            persist: true
        )
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m3", body: .text(text: "lost"), replyTo: "nowhere")),
            persist: true
        )

        let contents = try await fixture.messages("lobby").map(\.content)
        let firstId = Chat.ProductMessageId.stored("m1", in: fixture.chatId("lobby"))
        #expect(contents == [
            .text("question"),
            .reply(.init(messageId: user.messageId, ownContent: .init(text: "answer", attachments: nil))),
            .reply(.init(messageId: firstId, ownContent: .init(text: "again", attachments: nil))),
            .text("lost")
        ])
    }

    /// A product cannot put words in the mouth of the user.
    @Test func aProductCannotPostAsTheUser() async throws {
        let bridge = try await openRoom()

        await #expect(throws: NativeChatFeedRejection.NotOwner) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .new(message("m1", body: .text(text: "hi"), participant: "user")),
                persist: true
            )
        }
        #expect(try await fixture.messages("lobby").isEmpty)
    }

    @Test func anIdThatTheRoomHoldsIsADuplicate() async throws {
        let bridge = try await openRoom()
        let user = fixture.userMessage(.text("hi"), roomId: "lobby")
        try await fixture.save(user)
        try await bridge.updateFeed(roomId: "lobby", update: .new(message("m1", body: .text(text: "a"))), persist: true)

        await #expect(throws: NativeChatFeedRejection.DuplicateMessage) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .new(message("m1", body: .text(text: "b"))),
                persist: true
            )
        }
        await #expect(throws: NativeChatFeedRejection.DuplicateMessage) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .new(message(user.messageId, body: .text(text: "c"))),
                persist: true
            )
        }
    }

    /// A product picks the id of a message in one room only, so two rooms can use the same id.
    @Test func twoRoomsCanHoldTheSameId() async throws {
        let bridge = try await openRoom()
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "other", name: "Other", icon: nil)

        try await bridge.updateFeed(roomId: "lobby", update: .new(message("m1", body: .text(text: "a"))), persist: true)
        try await bridge.updateFeed(roomId: "other", update: .new(message("m1", body: .text(text: "b"))), persist: true)

        #expect(try await fixture.messages("lobby").map(\.content) == [.text("a")])
        #expect(try await fixture.messages("other").map(\.content) == [.text("b")])
    }

    // MARK: - Edited

    @Test func anEditReplacesTheContentAndMarksTheMessageEdited() async throws {
        let bridge = try await openRoom()
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "draft"))),
            persist: true
        )

        for text in ["draft 2", "final"] {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .edited(messageId: "m1", body: .text(text: text), editedAt: 1_700_000_000_500),
                persist: true
            )
        }

        let messages = try await fixture.messages("lobby")
        let edited = try #require(messages.first { $0.messageId.hasSuffix("/m1") })
        #expect(edited.content == .text("final"))
        let edits = messages.compactMap { message -> String? in
            guard case let .edited(edit) = message.content else { return nil }
            return edit.newContent.text
        }
        #expect(edits == ["final"])
        #expect(try await fixture.storedChat("lobby")?.message?.content == .text("final"))
    }

    /// A message that the product draws again in place shows no "edited" (`Replaced`).
    @Test func aReplacedMessageDrawsAgainWithoutAnEdit() async throws {
        let bridge = try await openRoom()
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "Waiting for the other player"))),
            persist: true
        )

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .replaced(message("m1", body: .text(text: "The game started"))),
            persist: true
        )

        #expect(try await fixture.messages("lobby").map(\.content) == [.text("The game started")])
    }

    @Test func aProductCannotEditAMessageOfTheUser() async throws {
        let bridge = try await openRoom()
        let user = fixture.userMessage(.text("mine"), roomId: "lobby")
        try await fixture.save(user)

        await #expect(throws: NativeChatFeedRejection.NotOwner) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .edited(messageId: user.messageId, body: .text(text: "yours"), editedAt: 1),
                persist: true
            )
        }
        #expect(try await fixture.messages("lobby").map(\.content) == [.text("mine")])
    }

    @Test func anEditOfAMessageThatIsNotThereIsUnknown() async throws {
        let bridge = try await openRoom()

        await #expect(throws: NativeChatFeedRejection.UnknownMessage) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .edited(messageId: "m9", body: .text(text: "x"), editedAt: 1),
                persist: true
            )
        }
    }

    // MARK: - Reactions

    @Test func reactionsOfProductParticipantsAddUpWithTheUser() async throws {
        let bridge = try await openRoom()
        let user = fixture.userMessage(.text("hi"), roomId: "lobby")
        try await fixture.save(user)

        for participant in ["product", "alice", "bob"] {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .reactionPlaced(messageId: user.messageId, participantId: participant, emoji: "👍"),
                persist: true
            )
        }
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .reactionPlaced(messageId: user.messageId, participantId: "alice", emoji: "👍"),
            persist: true
        )
        #expect(try await reactions(of: user.messageId) == ["👍", "👍", "👍"])

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .reactionRemoved(messageId: user.messageId, participantId: "bob", emoji: "👍"),
            persist: true
        )
        #expect(try await reactions(of: user.messageId) == ["👍", "👍"])
        #expect(try await fixture.messages("lobby").first?.reactions.allSatisfy {
            $0.origin == .chatExtension(fixture.productId)
        } == true)
    }

    @Test func aProductCannotReactAsTheUserOrToAMessageThatIsNotThere() async throws {
        let bridge = try await openRoom()
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "hi"))),
            persist: true
        )

        await #expect(throws: NativeChatFeedRejection.NotOwner) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .reactionPlaced(messageId: "m1", participantId: "user", emoji: "👍"),
                persist: true
            )
        }
        await #expect(throws: NativeChatFeedRejection.UnknownMessage) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .reactionPlaced(messageId: "m9", participantId: "alice", emoji: "👍"),
                persist: true
            )
        }
        #expect(try await reactions(of: Chat.ProductMessageId.stored("m1", in: fixture.chatId("lobby"))).isEmpty)
    }

    // MARK: - Status

    @Test(arguments: [
        (ChatMessageStatus.sent, Chat.LocalMessage.Status.outgoing(.sent)),
        (.delivered, .outgoing(.delivered)),
        (.read, .outgoing(.delivered)),
        (.failed, .outgoing(.failed)),
        (.preparing, .outgoing(.new))
    ])
    func aStatusChangesAMessageOfTheUser(status: ChatMessageStatus, stored: Chat.LocalMessage.Status) async throws {
        let bridge = try await openRoom()
        let user = fixture.userMessage(.text("hi"), roomId: "lobby")
        try await fixture.save(user)

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .statusChanged(messageId: user.messageId, status: status),
            persist: true
        )

        #expect(try await fixture.messages("lobby").first?.status == stored)
    }

    /// The host keeps the read state of a product message, so a status for it changes nothing.
    @Test func aStatusLeavesAProductMessageAlone() async throws {
        let bridge = try await openRoom()
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "hi"))),
            persist: true
        )

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .statusChanged(messageId: "m1", status: .read),
            persist: true
        )

        #expect(try await fixture.messages("lobby").first?.status == .incoming(.new))
        await #expect(throws: NativeChatFeedRejection.UnknownMessage) {
            try await bridge.updateFeed(
                roomId: "lobby",
                update: .statusChanged(messageId: "m9", status: .read),
                persist: true
            )
        }
    }

    // MARK: - Rooms

    @Test func anUpdateForARoomThatDoesNotExistIsRefused() async throws {
        let bridge = await fixture.bridge()

        await #expect(throws: NativeChatFeedRejection.self) {
            try await bridge.updateFeed(
                roomId: "gone",
                update: .statusChanged(messageId: "m1", status: .sent),
                persist: true
            )
        }
    }

    /// A room that the product keeps stores nothing.
    @Test func anUpdateThatIsNotKeptStoresNothing() async throws {
        let bridge = try await openRoom()

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "hi"))),
            persist: false
        )

        #expect(try await fixture.messages("lobby").isEmpty)
    }
}

/// One body of a `New` update and the content that the store keeps for it.
struct BodyCase: CustomTestStringConvertible, Sendable {
    let testDescription: String
    let body: ChatMessageBody
    let content: Chat.LocalMessage.Content

    private static let hop = TrUAPIChatResource.hop(
        identifier: Data(repeating: 1, count: 32),
        ticket: Data(repeating: 2, count: 32),
        node: "wss://hop",
        size: 3
    )

    private static let meta = ChatMediaMeta(
        mimeType: "image/png",
        sizeBytes: 3,
        width: 4,
        height: 5,
        placeholder: "L",
        alt: nil,
        durationMs: nil
    )

    static let all: [BodyCase] = [
        BodyCase(testDescription: "text", body: .text(text: "plain"), content: .text("plain")),
        BodyCase(
            testDescription: "caption without media",
            body: .richText(text: "**bold**", media: []),
            content: .text("**bold**")
        ),
        // This host draws the caption of a media body, and no photo or video.
        BodyCase(
            testDescription: "photo with a caption",
            body: .richText(text: "look", media: [.image(resource: hop, meta: meta)]),
            content: .text("look")
        ),
        BodyCase(
            testDescription: "photo without a caption",
            body: .richText(text: nil, media: [.image(resource: hop, meta: meta)]),
            content: .unsupported(nil)
        ),
        BodyCase(
            testDescription: "accepted",
            body: .event(.requestAccepted),
            content: .chatAccepted(.init(messageId: ""))
        ),
        BodyCase(testDescription: "contact added", body: .event(.contactAdded), content: .contactAdded),
        BodyCase(testDescription: "left", body: .event(.leftChat), content: .leftChat),
        BodyCase(testDescription: "unsupported", body: .event(.unsupported), content: .unsupported(nil)),
        BodyCase(
            testDescription: "call",
            body: .call(ChatCallRecord(video: false, answeredAt: nil, closed: nil)),
            content: .unsupported(nil)
        ),
        BodyCase(
            testDescription: "payment",
            body: .payment(ChatPaymentRecord(amount: "42", legacy: false)),
            content: .unsupported(nil)
        ),
        // The product draws a custom body, as it draws a custom message that it posts.
        BodyCase(
            testDescription: "custom",
            body: .custom(message: ChatCustomMessage(messageType: "poll", payload: Data([1, 2])), alt: "A poll"),
            content: .customRendered(
                .init(
                    decoderId: MessageDecoderIdentifier.product.rawValue,
                    data: Data([1, 2]),
                    identifier: "poll",
                    alt: "A poll"
                )
            )
        )
    ]
}

private extension ChatProductFeedTests {
    func openRoom() async throws -> RustChatExecutionBridge {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
        return await fixture.bridge()
    }

    func message(
        _ id: String,
        body: ChatMessageBody,
        participant: String = "product",
        replyTo: String? = nil
    ) -> ChatMessage {
        ChatMessage(
            messageId: id,
            origin: ChatMessageOrigin(participantId: participant),
            timestamp: 1_700_000_000_000,
            body: body,
            status: .sent,
            replyTo: replyTo,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
    }

    func reactions(of storedId: String) async throws -> [String] {
        try await fixture.messages("lobby").first { $0.messageId == storedId }?.reactions.map(\.emoji) ?? []
    }
}
