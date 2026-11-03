import CoreData
import Foundation
import MessageExchangeKit
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

        try await makeService().acceptOutgoingRequest(
            requestId,
            remoteIncoming: nil,
            acceptorDevice: nil,
            peerDevice: acceptorDevice
        )

        let state = try await readState()
        #expect(state.devices == [acceptorDevice])
        #expect(state.hasPendingRequest == false)
        #expect(state.messageStatus == Chat.LocalMessage.Status.outgoing(.delivered).rawValue)
    }
}

private extension AcceptOutgoingRequestDeviceTests {
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

    func insertPendingOutgoingRequest() async throws {
        try await facade.databaseService.performWrite { [peer, requestId] context in
            let contact = CDChatContact(context: context)
            contact.identifier = peer.toHex()
            contact.username = "peer"
            contact.publicKey = Data(repeating: 1, count: 32)
            contact.ownSignKeyId = "//wallet//main"

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
