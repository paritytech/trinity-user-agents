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

        @Test("an incoming message the user saw is marked seen")
        func incomingSeen() async throws {
            try await chatManager.setup()

            let msg = try await chatManager.sendMessage(.text("hi"), status: .incoming(.new))

            try await updateStatus(msg.messageId, to: .incoming(.seen))

            let fetched = try await fetchMessage(msg.messageId)
            #expect(fetched?.status == .incoming(.seen))
        }

        @Test("an outgoing status update sets the status of the message")
        func outgoingStatus() async throws {
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
