import Foundation
import Operation_iOS
import StructuredConcurrency
import Testing
@testable import polkadot_app

struct ChatRequestRenewalStoreTests {
    private let facade = UserDataStorageTestFacade()

    @Test func listsAnonymouslyDeliveredPendingRequestsNewestFirst() async throws {
        let store = ChatRequestDeliveryStore(storageFacade: facade)
        try await recordRequest("older", peer: 1, timestamp: 1)
        try await recordRequest("newer", peer: 2, timestamp: 2)
        try await store.markDelivered(requestId: "older", anonymousPeriod: 99)
        try await store.markDelivered(requestId: "newer", anonymousPeriod: 100)

        let candidates = try await store.renewalCandidates()

        #expect(candidates.map(\.message.messageId) == ["newer", "older"])
        #expect(candidates.map(\.period) == [100, 99])
        #expect(candidates.first?.session.peer.accountId == Data(repeating: 2, count: 32))
    }

    @Test func skipsRequestsNotYetDelivered() async throws {
        try await recordRequest("pending", peer: 1, timestamp: 1)

        #expect(try await ChatRequestDeliveryStore(storageFacade: facade).renewalCandidates().isEmpty)
    }

    @Test func updatesPeriodOfRenewedRequest() async throws {
        let store = ChatRequestDeliveryStore(storageFacade: facade)
        try await recordRequest("request", peer: 1, timestamp: 1)
        try await store.markDelivered(requestId: "request", anonymousPeriod: 99)

        try await store.updateAnonymousPeriod(requestId: "request", period: 100)

        #expect(try await store.renewalCandidates().map(\.period) == [100])
    }

    private func recordRequest(_ requestId: String, peer: UInt8, timestamp: UInt64) async throws {
        let newOutgoing = try ChatRequest.NewOutgoing(
            message: Chat.RequestMessage(
                messageId: requestId,
                timestamp: timestamp,
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            remoteContact: Chat.RemoteContact(
                accountId: Data(repeating: peer, count: 32),
                username: "peer\(peer)",
                chatPublicKey: Chat.PublicKey(rawData: Data(repeating: peer, count: 32)),
                imageData: nil
            ),
            pushId: nil,
            ownKeyId: Chat.Contact.Own(signKeyId: "//wallet//main", encryptionKeyId: "main")
        )

        try await facade.createRepository(mapper: AnyCoreDataMapper(NewOutgoingChatRequestMapper()))
            .saveOperation({ [newOutgoing] }, { [] })
            .asyncExecute()
    }
}
