import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

@Suite("UnreadMessageCountService")
struct UnreadMessageCountServiceTests {
    private let facade = UserDataStorageTestFacade()
    private let alice: ChatExtension.Id = "alice.dot"
    private let bob: ChatExtension.Id = "bob.dot"
    private let charlie: ChatExtension.Id = "charlie.dot"

    @Test("counts unread badge messages across chats and excludes reactions and system messages")
    func countsUnreadBadgeMessagesAcrossChatsExcludingReactionsAndSystemMessages() async throws {
        try await seedChats()
        try await seedMessages()

        let service = UnreadMessageCountService(databaseService: facade.databaseService)

        let count = try await service.totalUnreadBadgeMessageCount()

        #expect(count == 3)
    }

    /// The person chats the native chat left behind belong to the migration: the chat product's rooms carry them.
    @Test("does not count the person chats of the native chat")
    func doesNotCountNativePersonChats() async throws {
        try await seedChats()
        try await seedMessages()
        let native = Chat.Contact(
            accountId: Data(repeating: 0x09, count: 32),
            username: "Native",
            publicKey: Data(repeating: 0x0A, count: 32),
            ownKeyId: .init(signKeyId: "native-sign-key", encryptionKeyId: "native-encryption-key"),
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
        try await facade.makeRepo(mapper: ChatContactMapper()).saveOperation({ [native] }, { [] }).asyncExecute()
        try await chatRepository.saveOperation({ [.newChatWithContact(native)] }, { [] }).asyncExecute()
        try await messageRepository.saveOperation({
            [Chat.LocalMessage(
                messageId: "native-message",
                chatId: .person(native.accountId),
                origin: .contact(native.accountId),
                creationSource: .localDevice,
                status: .incoming(.new),
                timestamp: 0,
                content: .text("native"),
                reactions: [],
                compactionId: nil,
                relatedMessages: []
            )]
        }, { [] }).asyncExecute()

        let service = UnreadMessageCountService(databaseService: facade.databaseService)

        #expect(try await service.totalUnreadBadgeMessageCount() == 3)
    }

    @Test("counts a notified but unsaved message once")
    func countsUnsavedMessageOnce() async throws {
        try await seedChats()
        try await seedMessages()

        let service = UnreadMessageCountService(databaseService: facade.databaseService)

        let count = try await service.totalUnreadBadgeMessageCount(unsavedMessageId: "stripped-message")

        #expect(count == 4)
    }

    @Test("does not add an unsaved message that already has a row")
    func doesNotDoubleCountSavedMessage() async throws {
        try await seedChats()
        try await seedMessages()

        let service = UnreadMessageCountService(databaseService: facade.databaseService)

        let count = try await service.totalUnreadBadgeMessageCount(unsavedMessageId: "text-message")

        #expect(count == 3)
    }
}

private extension UnreadMessageCountServiceTests {
    var chatRepository: AnyDataProviderRepository<Chat.LocalModel> {
        facade.makeRepo(mapper: ChatModelMapper())
    }

    var messageRepository: AnyDataProviderRepository<Chat.LocalMessage> {
        facade.makeRepo(mapper: ChatMessageEntityMapper())
    }

    func seedChats() async throws {
        try await chatRepository
            .saveOperation({
                [
                    .newChatWithExtension(alice),
                    .newChatWithExtension(bob),
                    .newChatWithExtension(charlie)
                ]
            }, { [] })
            .asyncExecute()
    }

    func seedMessages() async throws {
        try await messageRepository.saveOperation({
            [
                makeMessage(
                    messageId: "text-message",
                    chat: alice,
                    status: .incoming(.new),
                    content: .text("first")
                ),
                makeMessage(
                    messageId: "second-text-message",
                    chat: alice,
                    status: .incoming(.new),
                    content: .text("second")
                ),
                makeMessage(
                    messageId: "reaction-message",
                    chat: alice,
                    status: .incoming(.new),
                    content: .reacted(.init(messageId: "text-message", emoji: "ok"))
                ),
                makeMessage(
                    messageId: "system-token-message",
                    chat: alice,
                    status: .incoming(.new),
                    content: .token(.init(token: Data(repeating: 0x07, count: 8), pushType: .ios))
                ),
                makeMessage(
                    messageId: "seen-message",
                    chat: alice,
                    status: .incoming(.seen),
                    content: .text("seen")
                ),
                makeMessage(
                    messageId: "other-chat-text-message",
                    chat: bob,
                    status: .incoming(.new),
                    content: .text("other chat")
                ),
                makeMessage(
                    messageId: "chat-without-unread-message",
                    chat: charlie,
                    status: .incoming(.seen),
                    content: .text("already seen")
                )
            ]
        }, {
            []
        }).asyncExecute()
    }

    func makeMessage(
        messageId: String,
        chat: ChatExtension.Id,
        status: Chat.LocalMessage.Status,
        content: Chat.LocalMessage.Content
    ) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: messageId,
            chatId: .chatExtension(chat, roomId: nil),
            origin: .chatExtension(chat),
            creationSource: .localDevice,
            status: status,
            timestamp: 0,
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }
}
