@testable import polkadot_app
import Foundation
import SubstrateSdk
import Testing

struct IncomingChatRequestCancelTests {
    private let peerAccountId = Data(repeating: 0x07, count: 32)
    private let otherAccountId = Data(repeating: 0x08, count: 32)
    private let ownKeyId = Chat.Contact.Own(signKeyId: "//wallet//main", encryptionKeyId: "")

    @Test("A cancel removes the pending incoming request without cleaning up the chat")
    func cancelRemovesPendingRequest() async throws {
        let store = MockChatRequestStoreService(
            localRequests: ["r1": makeLocalRequest(status: .incoming(.new), timestamp: 1)]
        )
        let cleanup = SpyChatRemovalCleanupService()
        let context = try await makeContext(store: store, cleanup: cleanup, contacts: [makeContact()])

        try await context.handle(
            incomingRequest: makeCancel(requestId: "r1", timestamp: 2),
            ownKeyId: ownKeyId
        )

        #expect(store.calls == [.cancelIncoming(requestId: "r1")])
        #expect(cleanup.accountIds.isEmpty)
    }

    @Test("A cancel for an accepted request removes the empty chat and cleans up the peer")
    func cancelRemovesEmptyAcceptedChat() async throws {
        let store = MockChatRequestStoreService(
            localRequests: ["r1": makeLocalRequest(status: .incoming(.accepted), timestamp: 1)]
        )
        let cleanup = SpyChatRemovalCleanupService()
        let context = try await makeContext(store: store, cleanup: cleanup)

        try await context.handle(
            incomingRequest: makeCancel(requestId: "r1", timestamp: 2),
            ownKeyId: ownKeyId
        )

        #expect(store.calls == [.cancelIncoming(requestId: "r1")])
        #expect(cleanup.accountIds == [peerAccountId])
    }

    @Test("A cancel for an accepted request with a conversation keeps the chat")
    func cancelKeepsAcceptedChatWithConversation() async throws {
        let store = MockChatRequestStoreService(
            localRequests: ["r1": makeLocalRequest(status: .incoming(.accepted), timestamp: 1)],
            cancelResult: false
        )
        let cleanup = SpyChatRemovalCleanupService()
        let context = try await makeContext(store: store, cleanup: cleanup)

        try await context.handle(
            incomingRequest: makeCancel(requestId: "r1", timestamp: 2),
            ownKeyId: ownKeyId
        )

        #expect(store.calls == [.cancelIncoming(requestId: "r1")])
        #expect(cleanup.accountIds.isEmpty)
    }

    @Test("A cancel for an unknown request stores a tombstone")
    func cancelWithoutLocalRequestStoresTombstone() async throws {
        let store = MockChatRequestStoreService()
        let context = try await makeContext(store: store)

        try await context.handle(
            incomingRequest: makeCancel(requestId: "r1", timestamp: 2),
            ownKeyId: ownKeyId
        )

        #expect(store.calls == [.storeCancelled(requestId: "r1")])
    }

    @Test("A stale copy of a cancelled request does not bring it back")
    func staleCopyAfterCancelIsIgnored() async throws {
        let store = MockChatRequestStoreService(
            localRequests: ["r1": makeLocalRequest(status: .incoming(.cancelled), timestamp: 2)]
        )
        let context = try await makeContext(store: store)

        try await context.handle(
            incomingRequest: makeRemoteRequest(requestId: "r1", timestamp: 1),
            ownKeyId: ownKeyId
        )

        #expect(store.calls.isEmpty)
    }

    @Test("A cancel for a declined or outgoing request is ignored")
    func cancelIgnoredForDeclinedOrOutgoingRequest() async throws {
        let statuses: [Chat.RequestStatus] = [.incoming(.declined), .outgoing(.pending)]

        for status in statuses {
            let store = MockChatRequestStoreService(
                localRequests: ["r1": makeLocalRequest(status: status, timestamp: 1)]
            )
            let context = try await makeContext(store: store)

            try await context.handle(
                incomingRequest: makeCancel(requestId: "r1", timestamp: 2),
                ownKeyId: ownKeyId
            )

            #expect(store.calls.isEmpty)
        }
    }

    @Test("A cancel with the same timestamp as the pending request is ignored")
    func cancelWithEqualTimestampIsIgnored() async throws {
        let store = MockChatRequestStoreService(
            localRequests: ["r1": makeLocalRequest(status: .incoming(.new), timestamp: 2)]
        )
        let context = try await makeContext(store: store, contacts: [makeContact()])

        try await context.handle(
            incomingRequest: makeCancel(requestId: "r1", timestamp: 2),
            ownKeyId: ownKeyId
        )

        #expect(store.calls.isEmpty)
    }

    @Test("A cancel from a peer that does not own the pending request is ignored")
    func cancelFromDifferentPeerIsIgnored() async throws {
        let store = MockChatRequestStoreService(
            localRequests: [
                "r1": makeLocalRequest(status: .incoming(.new), timestamp: 1, contactAccountId: otherAccountId)
            ]
        )
        let context = try await makeContext(store: store, contacts: [makeContact()])

        try await context.handle(
            incomingRequest: makeCancel(requestId: "r1", timestamp: 2),
            ownKeyId: ownKeyId
        )

        #expect(store.calls.isEmpty)
    }

    @Test("A cancel for an unknown request never accepts the outgoing request of the same contact")
    func cancelDoesNotTouchOutgoingRequest() async throws {
        let store = MockChatRequestStoreService()
        let outgoing = makeLocalRequest(status: .outgoing(.pending), timestamp: 1, requestId: "out")
        let context = try await makeContext(
            store: store,
            contacts: [makeContact(chatRequest: outgoing)]
        )

        try await context.handle(
            incomingRequest: makeCancel(requestId: "r9", timestamp: 2),
            ownKeyId: ownKeyId
        )

        #expect(store.calls == [.storeCancelled(requestId: "r9")])
    }
}

private extension IncomingChatRequestCancelTests {
    func makeContext(
        store: MockChatRequestStoreService,
        cleanup: SpyChatRemovalCleanupService = SpyChatRemovalCleanupService(),
        contacts: [Chat.Contact] = []
    ) async throws -> IncomingChatRequestCoordinationContext {
        let context = IncomingChatRequestCoordinationContext(
            discoveryOwnKeyIds: [],
            matchOwnKeyIds: [],
            requestStoreService: store,
            contactsStorageService: ContactsLocalStorageService(
                storageFacade: UserDataStorageTestFacade(),
                logger: MockLogger()
            ),
            remoteContactResolver: MockRemoteContactResolver(result: .success(nil)),
            chatRemovalCleanup: cleanup
        )

        await context.update(
            contacts: contacts,
            discoverTaskBuilder: { _ in Task {} },
            incomingRequestTaskBuilder: { _, _ in Task {} }
        )

        return context
    }

    func makeContact(chatRequest: Chat.Request? = nil) -> Chat.Contact {
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
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
    }

    func makeLocalRequest(
        status: Chat.RequestStatus,
        timestamp: UInt64,
        requestId: String = "r1",
        contactAccountId: AccountId? = nil
    ) -> Chat.Request {
        Chat.Request(
            requestId: requestId,
            contactAccountId: contactAccountId ?? peerAccountId,
            timestamp: timestamp,
            status: status,
            message: nil
        )
    }

    func makeRemoteRequest(requestId: String, timestamp: UInt64) -> ChatRequest.ValidatedRemoteModel {
        ChatRequest.ValidatedRemoteModel(
            message: Chat.RequestMessage(
                messageId: requestId,
                timestamp: timestamp,
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            peerAccountId: peerAccountId,
            peerStatementAccountId: peerAccountId
        )
    }

    func makeCancel(requestId: String, timestamp: UInt64) -> ChatRequest.ValidatedRemoteModel {
        let identityProof = Chat.IdentityProof(
            identityAccountId: Data(repeating: 0x07, count: 32),
            proof: Data(repeating: 0x09, count: 32)
        )

        return ChatRequest.ValidatedRemoteModel(
            message: Chat.RequestMessage(
                messageId: requestId,
                timestamp: timestamp,
                content: .cancel(Chat.RequestContentCancel(identityProof: identityProof))
            ),
            peerAccountId: peerAccountId,
            peerStatementAccountId: peerAccountId
        )
    }
}
