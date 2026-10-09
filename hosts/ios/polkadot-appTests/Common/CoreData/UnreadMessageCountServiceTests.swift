import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

@Suite("UnreadMessageCountService")
struct UnreadMessageCountServiceTests {
    private let facade = UserDataStorageTestFacade()
    private let alice = Chat.Contact(
        accountId: Data(repeating: 0x01, count: 32),
        username: "Alice",
        publicKey: Data(repeating: 0x02, count: 32),
        pin: nil,
        pushId: nil,
        pushToken: nil,
        voipPushToken: nil,
        peerPlatform: nil,
        lastOwnToken: nil,
        voipLastOwnToken: nil,
        chatRequest: nil,
        ownKeyId: .init(signKeyId: "sign-key", encryptionKeyId: "encryption-key"),
        imageData: nil,
        isBlocked: false,
        devices: [],
        pendingDevicesFanOut: false
    )
    private let bob = Chat.Contact(
        accountId: Data(repeating: 0x03, count: 32),
        username: "Bob",
        publicKey: Data(repeating: 0x04, count: 32),
        pin: nil,
        pushId: nil,
        pushToken: nil,
        voipPushToken: nil,
        peerPlatform: nil,
        lastOwnToken: nil,
        voipLastOwnToken: nil,
        chatRequest: nil,
        ownKeyId: .init(signKeyId: "bob-sign-key", encryptionKeyId: "bob-encryption-key"),
        imageData: nil,
        isBlocked: false,
        devices: [],
        pendingDevicesFanOut: false
    )
    private let charlie = Chat.Contact(
        accountId: Data(repeating: 0x05, count: 32),
        username: "Charlie",
        publicKey: Data(repeating: 0x06, count: 32),
        pin: nil,
        pushId: nil,
        pushToken: nil,
        voipPushToken: nil,
        peerPlatform: nil,
        lastOwnToken: nil,
        voipLastOwnToken: nil,
        chatRequest: nil,
        ownKeyId: .init(signKeyId: "charlie-sign-key", encryptionKeyId: "charlie-encryption-key"),
        imageData: nil,
        isBlocked: false,
        devices: [],
        pendingDevicesFanOut: false
    )

    @Test("counts unread badge messages across chats and excludes reactions and system messages")
    func countsUnreadBadgeMessagesAcrossChatsExcludingReactionsAndSystemMessages() async throws {
        try await seedChats()
        try await seedMessages()

        let service = UnreadMessageCountService(databaseService: facade.databaseService)

        let count = try await service.totalUnreadBadgeMessageCount()

        #expect(count == 3)
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
    var contactRepository: AnyDataProviderRepository<Chat.Contact> {
        facade.makeRepo(mapper: ChatContactMapper())
    }

    var chatRepository: AnyDataProviderRepository<Chat.LocalModel> {
        facade.makeRepo(mapper: ChatModelMapper())
    }

    var messageRepository: AnyDataProviderRepository<Chat.LocalMessage> {
        facade.makeRepo(mapper: ChatMessageEntityMapper())
    }

    func seedChats() async throws {
        try await contactRepository.saveOperation({ [alice, bob, charlie] }, { [] }).asyncExecute()
        try await chatRepository
            .saveOperation({
                [
                    .newChatWithContact(alice),
                    .newChatWithContact(bob),
                    .newChatWithContact(charlie)
                ]
            }, { [] })
            .asyncExecute()
    }

    func seedMessages() async throws {
        try await messageRepository.saveOperation({
            [
                makeMessage(
                    messageId: "text-message",
                    contact: alice,
                    status: .incoming(.new),
                    content: .text("first")
                ),
                makeMessage(
                    messageId: "second-text-message",
                    contact: alice,
                    status: .incoming(.new),
                    content: .text("second")
                ),
                makeMessage(
                    messageId: "reaction-message",
                    contact: alice,
                    status: .incoming(.new),
                    content: .reacted(.init(messageId: "text-message", emoji: "ok"))
                ),
                makeMessage(
                    messageId: "system-token-message",
                    contact: alice,
                    status: .incoming(.new),
                    content: .token(.init(token: Data(repeating: 0x07, count: 8), pushType: .ios))
                ),
                makeMessage(
                    messageId: "seen-message",
                    contact: alice,
                    status: .incoming(.seen),
                    content: .text("seen")
                ),
                makeMessage(
                    messageId: "other-chat-text-message",
                    contact: bob,
                    status: .incoming(.new),
                    content: .text("other chat")
                ),
                makeMessage(
                    messageId: "chat-without-unread-message",
                    contact: charlie,
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
        contact: Chat.Contact,
        status: Chat.LocalMessage.Status,
        content: Chat.LocalMessage.Content
    ) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: messageId,
            chatId: .person(contact.accountId),
            origin: .contact(contact.accountId),
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
