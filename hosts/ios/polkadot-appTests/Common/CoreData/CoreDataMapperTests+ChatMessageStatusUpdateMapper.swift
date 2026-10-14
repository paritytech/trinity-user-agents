import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

extension CoreDataMapperTests {
    @Suite("ChatMessageStatusUpdateMapper")
    struct ChatMessageStatusUpdateMapperTests {
        private let facade = UserDataStorageTestFacade()
        private let chatManager: TestChatManager

        init() {
            chatManager = TestChatManager(
                peer: MockChatPeer.person(),
                facade: facade
            )
        }

        @Test("outgoing status update propagates to compacted children")
        func propagatesToChildren() async throws {
            try await chatManager.setup()

            let msg1 = try await chatManager.sendMessage(.text("hello"), status: .outgoing(.new))
            let msg2 = try await chatManager.sendMessage(.text("world"), status: .outgoing(.new))

            let compactedMsg = try await saveCompactedMessage(
                originalIds: [msg1.messageId, msg2.messageId]
            )

            try await updateStatus(compactedMsg.messageId, to: .outgoing(.sent))

            let fetchedMsg1 = try await fetchMessage(msg1.messageId)
            let fetchedMsg2 = try await fetchMessage(msg2.messageId)

            #expect(fetchedMsg1?.status == .outgoing(.sent))
            #expect(fetchedMsg2?.status == .outgoing(.sent))
        }

        @Test("outgoing status update propagates recursively through nested compaction")
        func propagatesRecursively() async throws {
            try await chatManager.setup()

            let msg1 = try await chatManager.sendMessage(.text("a"), status: .outgoing(.new))
            let msg2 = try await chatManager.sendMessage(.text("b"), status: .outgoing(.new))

            let firstCompacted = try await saveCompactedMessage(
                originalIds: [msg1.messageId, msg2.messageId]
            )

            let msg3 = try await chatManager.sendMessage(.text("c"), status: .outgoing(.new))

            let secondCompacted = try await saveCompactedMessage(
                originalIds: [firstCompacted.messageId, msg3.messageId]
            )

            try await updateStatus(secondCompacted.messageId, to: .outgoing(.delivered))

            let fetchedFirst = try await fetchMessage(firstCompacted.messageId)
            let fetchedMsg1 = try await fetchMessage(msg1.messageId)
            let fetchedMsg2 = try await fetchMessage(msg2.messageId)
            let fetchedMsg3 = try await fetchMessage(msg3.messageId)

            #expect(fetchedFirst?.status == .outgoing(.delivered))
            #expect(fetchedMsg1?.status == .outgoing(.delivered))
            #expect(fetchedMsg2?.status == .outgoing(.delivered))
            #expect(fetchedMsg3?.status == .outgoing(.delivered))
        }

        @Test("incoming status update does not propagate to children")
        func incomingDoesNotPropagate() async throws {
            try await chatManager.setup()

            let msg1 = try await chatManager.sendMessage(.text("hi"), status: .incoming(.new))
            let msg2 = try await chatManager.sendMessage(.text("there"), status: .incoming(.new))

            let compactedMsg = try await saveCompactedMessage(
                originalIds: [msg1.messageId, msg2.messageId],
                status: .incoming(.new)
            )

            try await updateStatus(compactedMsg.messageId, to: .incoming(.seen))

            let fetchedMsg1 = try await fetchMessage(msg1.messageId)
            let fetchedMsg2 = try await fetchMessage(msg2.messageId)

            #expect(fetchedMsg1?.status == .incoming(.new))
            #expect(fetchedMsg2?.status == .incoming(.new))
        }

        @Test("outgoing status update with no children does nothing")
        func noChildrenNoOp() async throws {
            try await chatManager.setup()

            let msg = try await chatManager.sendMessage(.text("solo"), status: .outgoing(.new))

            try await updateStatus(msg.messageId, to: .outgoing(.sent))

            let fetched = try await fetchMessage(msg.messageId)
            #expect(fetched?.status == .outgoing(.sent))
        }
    }
}

// MARK: - Helpers

private extension CoreDataMapperTests.ChatMessageStatusUpdateMapperTests {
    var messageRepo: AnyDataProviderRepository<Chat.LocalMessage> {
        AnyDataProviderRepository(
            facade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageEntityMapper())
            )
        )
    }

    var statusRepo: AnyDataProviderRepository<Chat.ChatMessageStatusUpdate> {
        AnyDataProviderRepository(
            facade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageStatusUpdateMapper())
            )
        )
    }

    func saveCompactedMessage(
        originalIds: [String],
        status _: Chat.LocalMessage.Status = .outgoing(.new)
    ) async throws -> Chat.LocalMessage {
        let compactedContent = try ChatRemoteMessageContent.CompactedMessagesContent(
            claimIdentifier: Data.randomOrError(of: 32),
            claimTicket: Data.randomOrError(of: 32),
            node: .wssUrl("wss://test")
        )

        let remoteMessage = Chat.RemoteMessage(
            messageId: UUID().uuidString,
            timestamp: UInt64(Date().timeIntervalSince1970),
            versioned: .v1(.init(content: .compactedMessages(compactedContent)))
        )

        let commitModel = CompactionCommitMapper.Model(
            compactedRemoteMessage: remoteMessage,
            originalMessageIds: originalIds
        )

        let commitRepo: AnyDataProviderRepository<CompactionCommitMapper.Model> = AnyDataProviderRepository(
            facade.createRepository(
                mapper: AnyCoreDataMapper(CompactionCommitMapper())
            )
        )

        try await commitRepo.saveOperation({ [commitModel] }, { [] }).asyncExecute()

        let fetched = try #require(try await fetchMessage(remoteMessage.messageId))
        return fetched
    }

    func updateStatus(
        _ messageId: String,
        to status: Chat.LocalMessage.Status
    ) async throws {
        let update = Chat.ChatMessageStatusUpdate(messageId: messageId, status: status)
        try await statusRepo.saveOperation({ [update] }, { [] }).asyncExecute()
    }

    func fetchMessage(_ messageId: String) async throws -> Chat.LocalMessage? {
        try await messageRepo.fetchOperation(
            by: { messageId },
            options: .init()
        ).asyncExecute()
    }
}
