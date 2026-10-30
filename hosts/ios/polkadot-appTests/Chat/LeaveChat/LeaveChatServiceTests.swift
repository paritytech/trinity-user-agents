import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import Products
import SubstrateSdk
import Testing
@testable import polkadot_app

struct LeaveChatServiceTests {
    private let facade = UserDataStorageTestFacade()
    private let cleanupService = SpyAllowanceCleanupService()
    private let peerAccountId = Data(repeating: 0x01, count: 32)

    @Test func cleansUpAllowancesAfterContactIsDeleted() async throws {
        try await insertContact()
        var contactsAtCleanup: Int?
        cleanupService.onCleanup = { [self] in contactsAtCleanup = try? await contactCount() }

        try await makeService().leaveChat(.init(
            peer: .person(makeContact()),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil
        ))

        #expect(cleanupService.cleanupCount == 1)
        #expect(contactsAtCleanup == 0)
    }

    @Test func revokesIdentitySigningConsent() async throws {
        try await insertContact()
        try await consentStore().grant(for: peerAccountId)

        try await makeService().leaveChat(.init(
            peer: .person(makeContact()),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: nil
        ))

        #expect(try await !consentStore().hasConsent(for: peerAccountId))
    }

    /// A product room leaves the device the way a chat with a person does: the room and the
    /// messages it holds go, with nothing to send since the product reads its own room list. The
    /// product goes with its last room, or its worker would boot again only to recreate the room.
    /// No person is involved, so no allowance is released.
    @Test func leavingTheLastRoomDeletesItWithItsMessagesAndTheProduct() async throws {
        try await saveProduct()
        let room = try await saveRoom(roomId)
        let message = Chat.LocalMessage.newExtensionMessage(extensionId, roomId: roomId, content: .text("hello"))
        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()

        try await makeService().leaveChat(room)

        #expect(try await storedRoom(roomId) == nil)
        #expect(try await storedMessages().isEmpty)
        #expect(try await storedProduct() == nil)
        #expect(cleanupService.cleanupCount == 0)
    }

    /// The other rooms still need the product's worker, so the product stays installed.
    @Test func leavingOneRoomOfSeveralKeepsTheProduct() async throws {
        try await saveProduct()
        let room = try await saveRoom(roomId)
        try await saveRoom(otherRoomId)

        try await makeService().leaveChat(room)

        #expect(try await storedRoom(roomId) == nil)
        #expect(try await storedRoom(otherRoomId) != nil)
        #expect(try await storedProduct() != nil)
    }

    /// A native product can post without a room, into a chat that has no room metadata. That chat
    /// still needs the product, or it would linger hidden once the product is gone.
    @Test func leavingTheLastRoomKeepsTheProductWhileItsRoomlessChatRemains() async throws {
        try await saveProduct()
        let room = try await saveRoom(roomId)
        let roomlessChat = Chat.LocalModel.newChatWithExtension(extensionId)
        try await chatRepository.saveOperation({ [roomlessChat] }, { [] }).asyncExecute()

        try await makeService().leaveChat(room)

        #expect(try await storedRoom(roomId) == nil)
        #expect(try await storedProduct() != nil)
    }
}

private let extensionId: ChatExtension.Id = "coinflip.paseo"
private let roomId = "game-1"
private let otherRoomId = "game-2"

private extension LeaveChatServiceTests {
    func consentStore() -> ChatSigningConsentStore {
        ChatSigningConsentStore(storageFacade: facade, dateProvider: MockDateProvider())
    }

    func makeService() -> LeaveChatService {
        LeaveChatService(
            outboxService: StubLeaveOutbox(),
            allowanceCleanupService: cleanupService,
            signingConsentStore: consentStore(),
            productRepository: productRepository,
            storageFacade: facade
        )
    }

    func makeContact() -> Chat.Contact {
        Chat.Contact(
            accountId: peerAccountId,
            username: "peer",
            publicKey: Data(repeating: 0x02, count: 32),
            pin: nil,
            pushId: nil,
            pushToken: nil,
            voipPushToken: nil,
            peerPlatform: nil,
            lastOwnToken: nil,
            voipLastOwnToken: nil,
            chatRequest: nil,
            ownKeyId: .init(signKeyId: "//wallet//main", encryptionKeyId: "main"),
            imageData: nil,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
    }

    func insertContact() async throws {
        try await facade.databaseService.performWrite { [peerAccountId] context in
            let contact = CDChatContact(context: context)
            contact.identifier = peerAccountId.toHex()
            contact.username = "peer"
            contact.publicKey = Data(repeating: 0x02, count: 32)
            contact.ownSignKeyId = "//wallet//main"
        }
    }

    func contactCount() async throws -> Int {
        try await facade.databaseService.performRead { context in
            try context.count(for: CDChatContact.fetchRequest())
        }
    }

    var chatRepository: AnyDataProviderRepository<Chat.LocalModel> {
        AnyDataProviderRepository(facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper())))
    }

    /// Orders messages in a counter of its own, since the test host has no shared storage directory.
    var messageRepository: AnyDataProviderRepository<Chat.LocalMessage> {
        let counter = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let mapper = ChatMessageEntityMapper(orderAllocator: FileChatMessageOrderAllocator(fileURL: counter))
        return AnyDataProviderRepository(facade.createRepository(mapper: AnyCoreDataMapper(mapper)))
    }

    var productRepository: AnyDataProviderRepository<Product> {
        AnyDataProviderRepository(facade.createRepository(mapper: AnyCoreDataMapper(ProductMapper())))
    }

    func saveProduct() async throws {
        let product = Product(id: extensionId, name: "Coinflip")
        try await productRepository.saveOperation({ [product] }, { [] }).asyncExecute()
    }

    @discardableResult
    func saveRoom(_ roomId: String) async throws -> Chat.LocalModel {
        let room = Chat.LocalModel.newChatWithRoom(
            extensionId: extensionId,
            roomId: roomId,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: roomId, name: "Coinflip", icon: nil)
        )
        try await chatRepository.saveOperation({ [room] }, { [] }).asyncExecute()
        return room
    }

    func storedRoom(_ roomId: String) async throws -> Chat.LocalModel? {
        let chatId = Chat.Id.chatExtension(extensionId, roomId: roomId)
        return try await chatRepository
            .fetchOperation(by: { chatId.rawRepresentation }, options: RepositoryFetchOptions())
            .asyncExecute()
    }

    func storedMessages() async throws -> [Chat.LocalMessage] {
        try await messageRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
    }

    func storedProduct() async throws -> Product? {
        try await productRepository
            .fetchOperation(by: { extensionId }, options: RepositoryFetchOptions())
            .asyncExecute()
    }
}

private final class StubLeaveOutbox: ChatOutboxServicing {
    var exchangeService: AnyMessageExchangeService<Chat.OpaqueMessage>?

    func setupCallCoordinator(_: CallCoordinating) {}
    func setContactsByAccountId(_: [AccountId: Chat.Contact]) {}
    func syncPendingRemoteMessages(_: [Chat.RemoteMessage], for _: MessageExchange.Peer) {}
    func schedulePendingMessages() {}

    func handleSentMessages(
        _: [Chat.RemoteMessage],
        to _: Chat.Contact,
        withError _: MessageExchange.OutgoingMessageError?
    ) {}

    func handleDeliveredMessages(
        _: [Chat.RemoteMessage],
        to _: Chat.Contact,
        withError _: MessageExchange.OutgoingMessageError?
    ) {}

    func handleCompactedMessages(
        compactedMessage _: Chat.RemoteMessage,
        originalMessages _: [Chat.RemoteMessage],
        for _: MessageExchange.Peer
    ) {}

    func sendPeerLeftMessage(to _: Chat.Contact, completion: @escaping () -> Void) {
        completion()
    }

    func sendDirectly(_: Chat.RemoteMessage, to _: AccountId) {}
}
