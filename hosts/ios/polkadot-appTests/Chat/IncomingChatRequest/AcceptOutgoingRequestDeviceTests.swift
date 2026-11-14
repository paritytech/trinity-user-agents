import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import Testing
@testable import polkadot_app

struct AcceptOutgoingRequestDeviceTests {
    private let facade = UserDataStorageTestFacade()
    private let peer = Data(repeating: 0x07, count: 32)
    private let requestId = "request"
    private let acceptorDevice = Chat.PeerDevice(
        statementAccountId: Data(repeating: 0x0A, count: 32),
        encryptionPublicKey: Data(repeating: 0x0B, count: 32)
    )

    @Test func acceptanceStoresDeviceAndMarksDeliveredInOneSave() async throws {
        try await insertPendingOutgoingRequest()

        try await accept()

        let state = try await readState()
        #expect(state.devices == [acceptorDevice])
        #expect(state.hasPendingRequest == false)
        #expect(state.messageStatus == Chat.LocalMessage.Status.outgoing(.delivered).rawValue)
    }

    @Test("The acceptance is kept as the peer's message")
    func acceptanceIsStoredAsPeerMessage() async throws {
        try await insertPendingOutgoingRequest()

        try await accept()

        let acceptances = try await storedAcceptances()
        #expect(acceptances.count == 1)
        #expect(acceptances.first?.status == .incoming(.new))
        #expect(acceptances.first?.content == .multiChatAccepted(.init(requestId: requestId, device: acceptorDevice)))
    }

    @Test("An acceptance the notification extension already saved from its push isn't stored twice")
    func acceptanceSavedFromPushIsNotDuplicated() async throws {
        try await insertPendingOutgoingRequest()
        try await saveFromPush(acceptance)

        try await accept()

        #expect(try await storedAcceptances().count == 1)
    }

    @Test("Without a chat to hold the acceptance, the acceptor's device is still stored")
    func missingChatDoesNotLoseDevice() async throws {
        try await insertPendingOutgoingRequest(withChat: false)

        try await accept()

        #expect(try await readState().devices == [acceptorDevice])
        #expect(try await storedAcceptances().isEmpty)
    }
}

private extension AcceptOutgoingRequestDeviceTests {
    var acceptance: Chat.RemoteMessage {
        Chat.RemoteMessage(
            messageId: "acceptance",
            timestamp: 1_000,
            versioned: .v1(.init(content: .multiChatAccepted(.init(requestId: requestId, device: acceptorDevice))))
        )
    }

    func accept() async throws {
        try await makeService().acceptOutgoingRequest(
            requestId,
            remoteIncoming: nil,
            acceptorDevice: nil,
            peerAcceptance: ChatRequest.PeerAcceptance(device: acceptorDevice, message: acceptance)
        )
    }

    /// As `NewMessageHandler.save` stores a full push payload.
    func saveFromPush(_ message: Chat.RemoteMessage) async throws {
        let localMessage = try #require(
            Chat.LocalMessage(remote: message, creationSource: .localDevice, status: .incoming(.new), contactId: peer)
        )

        try await ChatMessageRepositoryFactory(storageFacade: facade)
            .createRepository(forFilter: nil)
            .saveOperation({ [localMessage] }, { [] })
            .asyncExecute()
    }

    func storedAcceptances() async throws -> [Chat.LocalMessage] {
        try await ChatMessageRepositoryFactory(storageFacade: facade)
            .createRepository(forFilter: .messageByContentType(.multiChatAccepted))
            .fetchAllOperation(with: .init())
            .asyncExecute()
    }

    struct State {
        let devices: [Chat.PeerDevice]
        let hasPendingRequest: Bool
        let messageStatus: Int16?
    }

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

    func insertPendingOutgoingRequest(withChat: Bool = true) async throws {
        try await facade.databaseService.performWrite { [peer, requestId] context in
            let contact = CDChatContact(context: context)
            contact.identifier = peer.toHex()
            contact.username = "peer"
            contact.publicKey = Data(repeating: 1, count: 32)
            contact.ownSignKeyId = "//wallet//main"

            if withChat {
                let chat = CDChat(context: context)
                chat.identifier = Chat.Id.person(peer).rawRepresentation
                chat.chatType = Int16(Chat.Id.ChatType.person.rawValue)
                chat.contact = contact
            }

            let message = CDChatMessage(context: context)
            message.messageId = requestId
            message.status = Chat.LocalMessage.Status.outgoing(.sent).rawValue

            let request = CDChatRequest(context: context)
            request.identifier = requestId
            request.contactId = peer.toHex()
            request.status = Chat.RequestStatus.outgoing.rawValue
            request.message = message
            request.contact = contact
        }
    }

    func readState() async throws -> State {
        try await facade.databaseService.performRead { [peer, requestId] context in
            let contactRequest = CDChatContact.fetchRequest()
            contactRequest.predicate = .contact(for: peer)
            let contact = try context.fetch(contactRequest).first
            let devices = (contact?.devices as? Set<CDContactDevice> ?? []).compactMap { device -> Chat.PeerDevice? in
                guard let accountId = device.statementAccountId, let key = device.encryptionPublicKey else {
                    return nil
                }
                return Chat.PeerDevice(statementAccountId: accountId, encryptionPublicKey: key)
            }

            let request: CDChatRequest? = try context.first(for: .chatRequestById(requestId))

            return State(
                devices: devices,
                hasPendingRequest: contact?.chatRequest != nil,
                messageStatus: request?.message?.status
            )
        }
    }
}
