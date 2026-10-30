import CoreData
import Foundation
import StatementDelivery
import Testing
@testable import polkadot_app

struct ChatAcceptanceOutgoingStoreTests {
    private let facade = UserDataStorageTestFacade()
    private let orderAllocator = InMemoryChatMessageOrderAllocator()
    private let peer = Data(repeating: 0x07, count: 32)
    private let requestId = "request"
    private let acceptanceTimestamp: UInt64 = 1_000
    private let acceptanceId = "acceptance"

    @Test func newAcceptanceAwaitsFirstDeliveryAndIsNotRenewed() async throws {
        try await insertAcceptance()
        let store = makeStore()

        #expect(try await store.isAwaitingFirstDelivery(input()))
        #expect(try await store.renewalCandidates().isEmpty)
    }

    @Test func firstPublicationMovesAcceptanceToRenewal() async throws {
        try await insertAcceptance()
        let store = makeStore()

        let publication = try await store.markPublished(input(), period: 42)

        let candidates = try await store.renewalCandidates()
        #expect(publication == StatementPublication(period: 42, isFirst: true))
        #expect(candidates.map(\.period) == [42])
        #expect(candidates.first?.input.requestId == requestId)
        #expect(candidates.first?.input.session.peer.accountId == peer)
        #expect(try await !store.isAwaitingFirstDelivery(input()))
    }

    @Test func renewedPublicationIsNotFirst() async throws {
        try await insertAcceptance()
        let store = makeStore()
        _ = try await store.markPublished(input(), period: 42)

        let publication = try await store.markPublished(input(), period: 43)

        #expect(publication == StatementPublication(period: 43, isFirst: false))
        #expect(try await store.renewalCandidates().map(\.period) == [43])
    }

    @Test func sessionSyncResetsSentMessagesButNotThePublishedAcceptance() async throws {
        try await insertAcceptance()
        let store = makeStore()
        _ = try await store.markPublished(input(), period: 42)
        let sentMessage = message(status: .outgoing(.sent), timestamp: acceptanceTimestamp + 1)
        try await insert(sentMessage)

        try await makeMessagesStorage()
            .markSentAsNewIfMissingIn(messageIds: [], contactId: peer)
            .asyncExecute()

        #expect(try await store.renewalCandidates().map(\.input.statementId) == [acceptanceId])
        #expect(try await !store.isAwaitingFirstDelivery(input()))
        #expect(try await status(of: sentMessage.messageId) == .outgoing(.new))
    }

    @Test func markAcknowledgedEndsRenewal() async throws {
        try await insertAcceptance()
        let store = makeStore()
        _ = try await store.markPublished(input(), period: 42)

        try await store.markAcknowledged(input())

        #expect(try await store.renewalCandidates().isEmpty)
    }

    @Test func markAbandonedFailsTheAcceptance() async throws {
        try await insertAcceptance()
        let store = makeStore()
        _ = try await store.markPublished(input(), period: 42)

        try await store.markAbandoned(input())

        #expect(try await store.renewalCandidates().isEmpty)
        #expect(try await !store.isAwaitingFirstDelivery(input()))
    }

    @Test func laterIncomingMessageAcknowledges() async throws {
        try await insertAcceptance()
        try await insertMessage(status: .incoming(.new), timestamp: acceptanceTimestamp + 1)

        #expect(try await isAcknowledged())
    }

    @Test func deliveredOutgoingMessageAcknowledges() async throws {
        try await insertAcceptance()
        try await insertMessage(status: .outgoing(.delivered), timestamp: acceptanceTimestamp + 1)

        #expect(try await isAcknowledged())
    }

    // Stored before the acceptance, so a peer clock ahead of ours must not make it an acknowledgement.
    @Test func earlierOrUnconfirmedTrafficDoesNotAcknowledge() async throws {
        try await insertAcceptance(storingFirst: message(status: .incoming(.seen), timestamp: acceptanceTimestamp + 1))
        try await insertMessage(status: .outgoing(.sent), timestamp: acceptanceTimestamp + 1)

        #expect(try await !isAcknowledged())
    }
}

private extension ChatAcceptanceOutgoingStoreTests {
    var chatId: Chat.Id { .person(peer) }

    func makeStore() -> ChatAcceptanceOutgoingStore {
        ChatAcceptanceOutgoingStore(storageFacade: facade)
    }

    func makeMessagesStorage() -> MessagesLocalStorageService {
        MessagesLocalStorageService(
            repositoryFactory: ChatMessageRepositoryFactory(storageFacade: facade),
            statusUpdateRepositoryFactory: ChatMessageStatusUpdateRepositoryFactory(storageFacade: facade),
            storageFacade: facade
        )
    }

    func status(of messageId: String) async throws -> Chat.LocalMessage.Status? {
        try await facade.databaseService.performRead { context in
            let fetchRequest = CDChatMessage.fetchRequest()
            fetchRequest.predicate = .messages(withIds: [messageId])

            return try context.fetch(fetchRequest).first.flatMap { Chat.LocalMessage.Status(rawValue: $0.status) }
        }
    }

    func isAcknowledged() async throws -> Bool {
        try await makeStore().isAcknowledged(input())
    }

    func input() -> OutgoingChatAcceptanceInput {
        OutgoingChatAcceptanceInput(
            message: acceptanceMessage(),
            requestId: requestId,
            contact: ChatAcceptanceFixture().makeContact()
        )
    }

    func acceptanceMessage() -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: acceptanceId,
            chatId: chatId,
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.new),
            timestamp: acceptanceTimestamp,
            content: .multiChatAccepted(.init(
                requestId: requestId,
                device: Chat.PeerDevice(
                    statementAccountId: Data(repeating: 2, count: 32),
                    encryptionPublicKey: Data(repeating: 3, count: 32)
                )
            )),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    func insertAcceptance(storingFirst earlierMessage: Chat.LocalMessage? = nil) async throws {
        let acceptance = acceptanceMessage()

        try await facade.databaseService.performWrite { [peer, chatId, requestId, orderAllocator] context in
            let contact = CDChatContact(context: context)
            contact.identifier = peer.toHex()
            contact.username = "peer"
            contact.publicKey = Data(repeating: 1, count: 32)
            contact.ownSignKeyId = "//wallet//main"

            let chat = CDChat(context: context)
            chat.identifier = chatId.rawRepresentation
            chat.chatType = Int16(Chat.Id.ChatType.person.rawValue)
            chat.contact = contact

            let request = CDChatRequest(context: context)
            request.identifier = requestId
            request.status = Chat.RequestStatus.incoming(.accepted).rawValue

            for message in [earlierMessage, acceptance].compactMap({ $0 }) {
                try ChatMessageEntityMapper(orderAllocator: orderAllocator).populate(
                    entity: CDChatMessage(context: context),
                    from: message,
                    using: context
                )
            }
        }
    }

    func message(status: Chat.LocalMessage.Status, timestamp: UInt64) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: UUID().uuidString,
            chatId: chatId,
            origin: .user,
            creationSource: .localDevice,
            status: status,
            timestamp: timestamp,
            content: .text("hi"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    func insertMessage(status: Chat.LocalMessage.Status, timestamp: UInt64) async throws {
        try await insert(message(status: status, timestamp: timestamp))
    }

    func insert(_ message: Chat.LocalMessage) async throws {
        try await facade.databaseService.performWrite { [orderAllocator] context in
            try ChatMessageEntityMapper(orderAllocator: orderAllocator).populate(
                entity: CDChatMessage(context: context),
                from: message,
                using: context
            )
        }
    }
}
