import Foundation
import MessageExchangeKit
import Operation_iOS
import Products
import SubstrateSdk
import Testing
@testable import polkadot_app

@Suite("Leave chat")
struct LeaveChatServiceTests {
    private let storageFacade = UserDataStorageTestFacade()

    /// A product room leaves the device the way a chat with a person does: the room and the
    /// messages it holds go, with nothing to send since the product reads its own room list. The
    /// product goes with its last room, or its worker would boot again only to recreate the room.
    @Test
    func leavingTheLastRoomDeletesItWithItsMessagesAndTheProduct() async throws {
        try await saveProduct()
        let room = try await saveRoom(roomId)
        let message = Chat.LocalMessage.newExtensionMessage(extensionId, roomId: roomId, content: .text("hello"))
        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()

        try await makeService().leaveChat(room)

        #expect(try await storedRoom(roomId) == nil)
        #expect(try await storedMessages().isEmpty)
        #expect(try await storedProduct() == nil)
    }

    /// The other rooms still need the product's worker, so the product stays installed.
    @Test
    func leavingOneRoomOfSeveralKeepsTheProduct() async throws {
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
    @Test
    func leavingTheLastRoomKeepsTheProductWhileItsRoomlessChatRemains() async throws {
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
    var chatRepository: AnyDataProviderRepository<Chat.LocalModel> {
        AnyDataProviderRepository(storageFacade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper())))
    }

    /// Orders messages in a counter of its own, since the test host has no shared storage directory.
    var messageRepository: AnyDataProviderRepository<Chat.LocalMessage> {
        let counter = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let mapper = ChatMessageEntityMapper(orderAllocator: FileChatMessageOrderAllocator(fileURL: counter))
        return AnyDataProviderRepository(storageFacade.createRepository(mapper: AnyCoreDataMapper(mapper)))
    }

    var productRepository: AnyDataProviderRepository<Product> {
        AnyDataProviderRepository(storageFacade.createRepository(mapper: AnyCoreDataMapper(ProductMapper())))
    }

    func makeService() -> LeaveChatService {
        LeaveChatService(
            outboxService: UnusedChatOutboxService(),
            messageExchangeModeProvider: IdentityModeProvider(),
            productRepository: productRepository,
            storageFacade: storageFacade
        )
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

private struct IdentityModeProvider: MessageExchangeModeProviding {
    let multideviceSignKeyIds: Set<String> = []

    func mode(forSignKeyId _: String) -> MessageExchangeMode {
        .identity
    }
}

/// Leaving a product room never reaches the outbox, so every call here is a test failure.
private final class UnusedChatOutboxService: ChatOutboxServicing {
    var exchangeService: AnyMessageExchangeService<Chat.OpaqueMessage>?

    func setupCallCoordinator(_: CallCoordinating) { Issue.record("unexpected outbox call") }

    func setContactsByAccountId(_: [AccountId: Chat.Contact]) { Issue.record("unexpected outbox call") }

    func syncPendingRemoteMessages(_: [Chat.RemoteMessage], for _: MessageExchange.Peer) {
        Issue.record("unexpected outbox call")
    }

    func schedulePendingMessages() { Issue.record("unexpected outbox call") }

    func handleSentMessages(
        _: [Chat.RemoteMessage],
        to _: Chat.Contact,
        withError _: MessageExchange.OutgoingMessageError?
    ) {
        Issue.record("unexpected outbox call")
    }

    func handleDeliveredMessages(
        _: [Chat.RemoteMessage],
        to _: Chat.Contact,
        withError _: MessageExchange.OutgoingMessageError?
    ) {
        Issue.record("unexpected outbox call")
    }

    func handleCompactedMessages(
        compactedMessage _: Chat.RemoteMessage,
        originalMessages _: [Chat.RemoteMessage],
        for _: MessageExchange.Peer
    ) {
        Issue.record("unexpected outbox call")
    }

    func sendPeerLeftMessage(to _: Chat.Contact, completion: @escaping () -> Void) {
        Issue.record("unexpected outbox call")
        completion()
    }

    func sendDirectly(_: Chat.RemoteMessage, to _: AccountId) { Issue.record("unexpected outbox call") }
}
