import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import Testing
@testable import polkadot_app

struct CancelIncomingChatRequestMapperTests {
    private let facade = UserDataStorageTestFacade()
    private let peer = Data(repeating: 0x07, count: 32)
    private let requestId = "request"
    private let ownKeyId = Chat.Contact.Own(signKeyId: "//wallet//main", encryptionKeyId: "main")
    private let acceptorDevice = Chat.PeerDevice(
        statementAccountId: Data(repeating: 0x0C, count: 32),
        encryptionPublicKey: Data(repeating: 0x0D, count: 32)
    )
    private let pushToken = Chat.RemoteTokenContent(token: Data(repeating: 0x03, count: 32), pushType: .ios)

    @Test func cancelOfPendingRequestRemovesContactAndChat() async throws {
        try await receiveRequest()

        #expect(try await makeService().cancelIncomingRequest(requestId))

        #expect(try await contactCount() == 0)
        #expect(try await chatCount() == 0)
        #expect(try await requestStatus() == .incoming(.cancelled))
        #expect(try await removedChatCount() == 0)
    }

    @Test func cancelOfAcceptedRequestWithOnlySystemMessagesRemovesContactAndChat() async throws {
        try await receiveRequest()
        try await acceptRequest()
        try await addMessage(content: .token(pushToken))

        #expect(try await makeService().cancelIncomingRequest(requestId))

        #expect(try await contactCount() == 0)
        #expect(try await chatCount() == 0)
        #expect(try await requestStatus() == .incoming(.cancelled))
        #expect(try await removedChatCount() == 1)
    }

    @Test func cancelOfAcceptedRequestWithConversationKeepsEverything() async throws {
        try await receiveRequest()
        try await acceptRequest()
        try await addMessage(content: .text("hello"))

        #expect(try await !makeService().cancelIncomingRequest(requestId))

        #expect(try await contactCount() == 1)
        #expect(try await chatCount() == 1)
        #expect(try await requestStatus() == .incoming(.accepted))
        #expect(try await removedChatCount() == 0)
    }

    @Test func cancelOfAcceptedRequestKeepsBlockedContact() async throws {
        try await receiveRequest()
        try await acceptRequest()
        try await addMessage(content: .token(pushToken))
        try await blockPeer()

        #expect(try await !makeService().cancelIncomingRequest(requestId))

        #expect(try await contactCount() == 1)
        #expect(try await chatCount() == 1)
        #expect(try await requestStatus() == .incoming(.accepted))
        #expect(try await removedChatCount() == 0)
        #expect(try await isPeerBlocked())
    }

    @Test func cancelOfPendingRequestKeepsBlockedContact() async throws {
        try await receiveRequest()
        try await blockPeer()

        #expect(try await !makeService().cancelIncomingRequest(requestId))

        #expect(try await contactCount() == 1)
        #expect(try await requestStatus() == .incoming(.new))
        #expect(try await isPeerBlocked())
    }
}

private extension CancelIncomingChatRequestMapperTests {
    func makeService() -> ChatRequestStoreService {
        ChatRequestStoreService(
            storageFacade: facade,
            pushIdFactory: ChatPushIdFactory(
                encryptionManager: ChatEncryptionManager(entropyManager: FixedRootEntropyManager()),
                signManager: ChatSignerManager(entropyManager: FixedRootEntropyManager()),
                sessionIdFactory: PeerSessionIdFactory(),
                logger: MockLogger()
            ),
            deviceEncryptionKeyManager: StubDeviceEncryptionKeyManager()
        )
    }

    func receiveRequest() async throws {
        let remote = ChatRequest.ValidatedRemoteModel(
            message: Chat.RequestMessage(
                messageId: requestId,
                timestamp: 1,
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            peerAccountId: peer,
            peerStatementAccountId: peer
        )
        let remoteContact = try Chat.RemoteContact(
            accountId: peer,
            username: "peer",
            chatPublicKey: Chat.PublicKey(rawData: Data(repeating: 0x02, count: 32)),
            imageData: nil
        )

        try await makeService().newIncomingRequestFromRemote(remote, contact: remoteContact, ownKeyId: ownKeyId)
    }

    func acceptRequest() async throws {
        try await makeService().acceptIncomingRequest(
            .existing(requestId: requestId, acceptorDevice: acceptorDevice)
        )
    }

    func addMessage(content: Chat.LocalMessage.Content) async throws {
        let message = Chat.LocalMessage.newMessageToPerson(peer, content: content)

        try await facade.createRepository(
            mapper: AnyCoreDataMapper(ChatMessageEntityMapper(orderAllocator: StubChatMessageOrderAllocator(value: 0)))
        )
        .saveOperation({ [message] }, { [] })
        .asyncExecute()
    }

    func blockPeer() async throws {
        let status = Chat.ContactBlockStatus(accountId: peer, isBlocked: true)

        try await facade.createRepository(mapper: AnyCoreDataMapper(BlockContactMapper()))
            .saveOperation({ [status] }, { [] })
            .asyncExecute()
    }

    func isPeerBlocked() async throws -> Bool {
        try await facade.databaseService.performRead { context in
            let contact: CDChatContact? = try context.first(for: .contact(for: self.peer))
            return contact?.isBlocked == true
        }
    }

    func requestStatus() async throws -> Chat.RequestStatus? {
        try await makeService().fetchByRequestId(requestId)?.status
    }

    func contactCount() async throws -> Int {
        try await facade.databaseService.performRead { context in
            try context.count(for: CDChatContact.fetchRequest())
        }
    }

    func chatCount() async throws -> Int {
        try await facade.databaseService.performRead { context in
            try context.count(for: CDChat.fetchRequest())
        }
    }

    func removedChatCount() async throws -> Int {
        try await facade.databaseService.performRead { context in
            try context.count(for: CDRemovedChat.fetchRequest())
        }
    }
}
