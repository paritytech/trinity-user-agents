import CoreData
import Foundation
import MessageExchangeKit
import Testing
@testable import polkadot_app

struct AcceptIncomingRequestDeviceTests {
    private let facade = UserDataStorageTestFacade()
    private let peer = Data(repeating: 0x07, count: 32)
    private let requestId = "request"
    private let requesterDevice = Chat.PeerDevice(
        statementAccountId: Data(repeating: 0x0A, count: 32),
        encryptionPublicKey: Data(repeating: 0x0B, count: 32)
    )
    private let acceptorDevice = Chat.PeerDevice(
        statementAccountId: Data(repeating: 0x0C, count: 32),
        encryptionPublicKey: Data(repeating: 0x0D, count: 32)
    )

    @Test func acceptStoresRequesterDeviceOnContact() async throws {
        try await insertPendingIncomingRequest()

        try await makeService().acceptIncomingRequest(
            .existing(requestId: requestId, acceptorDevice: acceptorDevice)
        )

        #expect(try await contactDevices() == [requesterDevice])
    }
}

private extension AcceptIncomingRequestDeviceTests {
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

    func insertPendingIncomingRequest() async throws {
        try await facade.databaseService.performWrite { [peer, requestId, requesterDevice] context in
            let contact = CDChatContact(context: context)
            contact.identifier = peer.toHex()
            contact.username = "peer"
            contact.publicKey = Data(repeating: 1, count: 32)
            contact.ownSignKeyId = "//wallet//main"

            let chat = CDChat(context: context)
            chat.identifier = Chat.Id.person(peer).rawRepresentation
            chat.chatType = Int16(Chat.Id.ChatType.person.rawValue)
            chat.contact = contact

            let request = CDChatRequest(context: context)
            request.identifier = requestId
            request.contactId = peer.toHex()
            request.status = Chat.RequestStatus.incoming(.new).rawValue
            request.contact = contact

            let device = CDChatRequestPeerDevice(context: context)
            device.requestId = requestId
            device.statementAccountId = requesterDevice.statementAccountId
            device.encryptionPublicKey = requesterDevice.encryptionPublicKey
        }
    }

    func contactDevices() async throws -> [Chat.PeerDevice] {
        try await facade.databaseService.performRead { [peer] context in
            let request = CDChatContact.fetchRequest()
            request.predicate = .contact(for: peer)
            let contact = try context.fetch(request).first
            let devices = contact?.devices as? Set<CDContactDevice> ?? []

            return devices.compactMap { device in
                guard let accountId = device.statementAccountId, let key = device.encryptionPublicKey else {
                    return nil
                }
                return Chat.PeerDevice(statementAccountId: accountId, encryptionPublicKey: key)
            }
        }
    }
}
