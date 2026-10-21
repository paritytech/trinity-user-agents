import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
import TrUAPIHost
@testable import polkadot_app

/// `update_feed` into a room that the host stores, through the rust chat bridge and the chat store (P4.1).
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

    @Test func aMessageOfAnotherParticipantKeepsTheParticipant() async throws {
        let bridge = try await openRoom()

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "hi"), participant: "alice")),
            persist: true
        )
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m2", body: .text(text: "hi"))),
            persist: true
        )

        #expect(try await participant(of: "m1") == "alice")
        #expect(try await participant(of: "m2") == nil)
    }

    /// 02-DESIGN §13 item 6: a product cannot put words in the mouth of the user.
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

    /// The product disables the buttons of an `Actions` message by an edit (02-DESIGN §6.3).
    @Test func anEditCanChangeTheBodyKind() async throws {
        let bridge = try await openRoom()
        let actions = ChatActions(text: "Pick", actions: [ChatAction(actionId: "a", title: "A")], layout: .column)
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .actions(actions))),
            persist: true
        )

        try await bridge.updateFeed(
            roomId: "lobby",
            update: .edited(
                messageId: "m1",
                body: .actions(ChatActions(text: "Picked A", actions: [], layout: .column)),
                editedAt: 1
            ),
            persist: true
        )

        #expect(try await fixture.messages("lobby").map(\.content) == [
            .productBody(.actions(.init(text: "Picked A", buttons: [], layout: .column)))
        ])
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

    // MARK: - Deleted

    @Test func aDeleteWritesTheTombstoneAndTheListShowsThePreviousMessage() async throws {
        let bridge = try await openRoom()
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m1", body: .text(text: "first"))),
            persist: true
        )
        try await bridge.updateFeed(
            roomId: "lobby",
            update: .new(message("m2", body: .text(text: "second"))),
            persist: true
        )

        try await bridge.updateFeed(roomId: "lobby", update: .deleted(messageId: "m2"), persist: true)
        try await bridge.updateFeed(roomId: "lobby", update: .deleted(messageId: "m2"), persist: true)

        #expect(try await fixture.messages("lobby").map(\.content) == [.text("first"), .deleted])
        #expect(try await fixture.storedChat("lobby")?.message?.content == .text("first"))
    }

    /// 02-DESIGN §13 item 6: a product can delete any message of the room.
    @Test func aProductCanDeleteAMessageOfTheUser() async throws {
        let bridge = try await openRoom()
        let user = fixture.userMessage(.text("oops"), roomId: "lobby")
        try await fixture.save(user)

        try await bridge.updateFeed(roomId: "lobby", update: .deleted(messageId: user.messageId), persist: true)

        #expect(try await fixture.messages("lobby").map(\.content) == [.deleted])
        await #expect(throws: NativeChatFeedRejection.UnknownMessage) {
            try await bridge.updateFeed(roomId: "lobby", update: .deleted(messageId: "m9"), persist: true)
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
        (.read, .outgoing(.read)),
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
            try await bridge.updateFeed(roomId: "gone", update: .deleted(messageId: "m1"), persist: true)
        }
    }

    /// A room that the product keeps stores nothing: an open room screen takes the update, else it goes nowhere.
    @Test func anUpdateThatIsNotKeptReachesOnlyAnOpenRoom() async throws {
        let listeners = ProductFeedListeners()
        let bridge = try await openRoom(feedListeners: listeners)
        let listener = RecordingFeedListener()
        let update = ChatFeedUpdate.new(message("m1", body: .text(text: "hi")))

        try await bridge.updateFeed(roomId: "lobby", update: update, persist: false)
        #expect(await listener.updates.isEmpty)

        let registration = listeners.register(listener, for: fixture.chatId("lobby"))
        try await bridge.updateFeed(roomId: "lobby", update: update, persist: false)
        try await bridge.updateFeed(roomId: "other", update: update, persist: false)
        registration.cancel()
        try await bridge.updateFeed(roomId: "lobby", update: update, persist: false)

        #expect(await listener.updates.map(\.roomId) == ["lobby"])
        #expect(await listener.updates.map(\.update) == [update])
        #expect(try await fixture.messages("lobby").isEmpty)
    }

    @Test func theRegistryDoesNotKeepAListener() async throws {
        let listeners = ProductFeedListeners()
        var listener: RecordingFeedListener? = RecordingFeedListener()
        let registration = try listeners.register(#require(listener), for: fixture.chatId("lobby"))
        listener = nil

        #expect(await listeners
            .deliver(.deleted(messageId: "m1"), roomId: "lobby", to: fixture.chatId("lobby")) == false)
        _ = registration
    }
}

/// One body of a `New` update and the content that the store keeps for it.
struct BodyCase: CustomTestStringConvertible, Sendable {
    let testDescription: String
    let body: ChatMessageBody
    let content: Chat.LocalMessage.Content

    private static let key = Data(repeating: 0xAB, count: 32)
    private static let keySource = "preimage:" + String(repeating: "ab", count: 32)
    private static let png = Data([0x89, 0x50])
    private static let pngSource = "data:image/png;base64," + Data([0x89, 0x50]).base64EncodedString()

    private static func meta(_ mimeType: String, size: UInt64? = nil) -> ChatMediaMeta {
        ChatMediaMeta(mimeType: mimeType, sizeBytes: size, width: nil, height: nil, placeholder: nil, alt: nil)
    }

    static let all: [BodyCase] = [
        BodyCase(testDescription: "text", body: .text(text: "plain"), content: .text("plain")),
        BodyCase(
            testDescription: "markdown",
            body: .richText(text: "**bold**", media: []),
            content: .productBody(.richText(.init(text: "**bold**", media: [])))
        ),
        BodyCase(
            testDescription: "inline and preimage images and a video",
            body: .richText(text: "look", media: [
                .image(resource: .inline(mimeType: "image/png", bytes: png), meta: meta("image/png")),
                .image(resource: .preimage(key: key), meta: meta("image/jpeg")),
                .video(resource: .preimage(key: key), meta: meta("video/mp4"))
            ]),
            content: .productBody(.richText(.init(
                text: "look",
                media: [pngSource, keySource, keySource],
                items: [
                    .init(kind: .image, mimeType: "image/png", fileName: nil, sizeBytes: nil),
                    .init(kind: .image, mimeType: "image/jpeg", fileName: nil, sizeBytes: nil),
                    .init(kind: .video, mimeType: "video/mp4", fileName: nil, sizeBytes: nil)
                ]
            )))
        ),
        BodyCase(
            testDescription: "file",
            body: .richText(text: "report", media: [
                .file(resource: .preimage(key: key), meta: meta("application/pdf", size: 2_048), fileName: "r.pdf")
            ]),
            content: .productBody(.file(.init(
                url: keySource,
                fileName: "r.pdf",
                mimeType: "application/pdf",
                sizeBytes: 2_048,
                text: "report"
            )))
        ),
        // PARITY row 4: the media bubble draws a file among the images as a row.
        BodyCase(
            testDescription: "file among images",
            body: .richText(text: nil, media: [
                .image(resource: .inline(mimeType: "image/png", bytes: png), meta: meta("image/png")),
                .file(resource: .preimage(key: key), meta: meta("application/pdf", size: 2_048), fileName: "r.pdf")
            ]),
            content: .productBody(.richText(.init(
                text: nil,
                media: [pngSource, keySource],
                items: [
                    .init(kind: .image, mimeType: "image/png", fileName: nil, sizeBytes: nil),
                    .init(kind: .file, mimeType: "application/pdf", fileName: "r.pdf", sizeBytes: 2_048)
                ]
            )))
        ),
        BodyCase(
            testDescription: "actions",
            body: .actions(ChatActions(
                text: "Pick",
                actions: [ChatAction(actionId: "y", title: "Yes")],
                layout: .grid
            )),
            content: .productBody(.actions(.init(
                text: "Pick",
                buttons: [.init(actionId: "y", title: "Yes")],
                layout: .grid
            )))
        ),
        BodyCase(
            testDescription: "custom",
            body: .custom(messageType: "score", payload: Data([1, 2]), previewText: "Score 2:1"),
            content: .customRendered(.init(
                decoderId: MessageDecoderIdentifier.product.rawValue,
                data: Data([1, 2]),
                identifier: "score",
                alt: "Score 2:1"
            ))
        )
    ]
}

private actor RecordingFeedListener: ProductFeedListening {
    private(set) var updates: [(roomId: String, update: ChatFeedUpdate)] = []

    func apply(roomId: String, update: ChatFeedUpdate) {
        updates.append((roomId, update))
    }
}

private extension ChatProductFeedTests {
    func openRoom(feedListeners: ProductFeedListeners = ProductFeedListeners()) async throws
        -> RustChatExecutionBridge {
        _ = try await fixture.context.createRoom(for: fixture.bot, roomId: "lobby", name: "Lobby", icon: nil)
        return await fixture.bridge(feedListeners: feedListeners)
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
            editedAt: nil,
            reactions: []
        )
    }

    func participant(of productMessageId: String) async throws -> String? {
        let storedId = Chat.ProductMessageId.stored(productMessageId, in: fixture.chatId("lobby"))
        return try await fixture.storageFacade.databaseService.performRead { context in
            let entity: CDChatMessage? = try context.first(for: .chatMessage(with: storedId))
            return entity?.originParticipantId
        }
    }

    func reactions(of storedId: String) async throws -> [String] {
        try await fixture.messages("lobby").first { $0.messageId == storedId }?.reactions.map(\.emoji) ?? []
    }
}

/// A product body keeps a preimage media item as a source string that the resource resolver reads back.
struct ChatResourcePreimageSourceTests {
    @Test func aPreimageSourceReadsBackAsItsKey() {
        let key = Data([0x00, 0x7F, 0xAB, 0xFF])

        #expect(ChatResource.preimageSource(key: key) == "preimage:007fabff")
        #expect(ChatResource(source: "preimage:007fabff") == .preimage(key: key))
    }

    @Test(arguments: ["preimage:", "preimage:abc", "preimage:zz", "preimages:00"])
    func aBrokenPreimageSourceIsNoResource(_ source: String) {
        #expect(ChatResource(source: source) == nil)
    }
}
