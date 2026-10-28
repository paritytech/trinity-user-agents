@testable import polkadot_app
import Foundation
import Testing

struct IncomingChatRequestBlockedContactTests {
    private let peerAccountId = Data(repeating: 0x07, count: 32)
    private let ownKeyId = Chat.Contact.Own(signKeyId: "//wallet//main", encryptionKeyId: "")

    @Test("A request from a blocked contact without a pending request is not auto-accepted")
    func blockedContactIsNotAutoAccepted() async throws {
        let store = MockChatRequestStoreService()
        let context = try await makeContext(store: store, contact: makeContact(isBlocked: true))

        try await context.handle(incomingRequest: makeRemoteRequest(), ownKeyId: ownKeyId)

        #expect(store.calls.isEmpty)
    }

    @Test("A crossed request from a blocked contact doesn't accept the outgoing one")
    func blockedContactCrossedRequestIsNotAccepted() async throws {
        let store = MockChatRequestStoreService()
        let outgoing = Chat.Request(
            requestId: "outgoing",
            contactAccountId: peerAccountId,
            timestamp: 1,
            status: .outgoing,
            message: nil
        )
        let context = try await makeContext(
            store: store,
            contact: makeContact(isBlocked: true, chatRequest: outgoing)
        )

        try await context.handle(incomingRequest: makeRemoteRequest(), ownKeyId: ownKeyId)

        #expect(store.calls.isEmpty)
    }

    @Test("A request from an unblocked contact without a pending request is still auto-accepted")
    func unblockedContactIsAutoAccepted() async throws {
        let store = MockChatRequestStoreService()
        let context = try await makeContext(store: store, contact: makeContact(isBlocked: false))

        try await context.handle(incomingRequest: makeRemoteRequest(), ownKeyId: ownKeyId)

        #expect(store.calls == [.acceptIncoming])
    }
}

private extension IncomingChatRequestBlockedContactTests {
    func makeContext(
        store: MockChatRequestStoreService,
        contact: Chat.Contact
    ) async throws -> IncomingChatRequestCoordinationContext {
        let context = IncomingChatRequestCoordinationContext(
            discoveryOwnKeyIds: [],
            matchOwnKeyIds: [],
            requestStoreService: store,
            contactsStorageService: ContactsLocalStorageService(
                storageFacade: UserDataStorageTestFacade(),
                logger: MockLogger()
            ),
            remoteContactResolver: MockRemoteContactResolver(result: .success(nil))
        )

        await context.update(
            contacts: [contact],
            discoverTaskBuilder: { _ in Task {} },
            incomingRequestTaskBuilder: { _, _ in Task {} }
        )

        return context
    }

    func makeContact(isBlocked: Bool, chatRequest: Chat.Request? = nil) -> Chat.Contact {
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
            chatRequest: chatRequest,
            ownKeyId: ownKeyId,
            imageData: nil,
            isBlocked: isBlocked,
            devices: [],
            pendingDevicesFanOut: false
        )
    }

    func makeRemoteRequest() -> ChatRequest.ValidatedRemoteModel {
        ChatRequest.ValidatedRemoteModel(
            message: Chat.RequestMessage(
                messageId: "incoming",
                timestamp: 2,
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            peerAccountId: peerAccountId,
            peerStatementAccountId: peerAccountId
        )
    }
}
