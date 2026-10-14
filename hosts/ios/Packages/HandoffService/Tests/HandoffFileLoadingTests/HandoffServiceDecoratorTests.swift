import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
@testable import HandoffService

struct HandoffServiceDecoratorTests {
    @Test func claimReturnsPoolDataWithoutFallback() async throws {
        let (decorator, service, remoteStore) = makeDecorator()

        let data = try Data.randomOrError(of: 100)
        let hash = try data.blake2b32()
        service.storage[hash] = data

        let claimed = try await decorator.claimData(by: hash, recipient: MockRecipientProofProvider())

        #expect(claimed == data)
        #expect(remoteStore.requestedHashes.isEmpty)
    }

    @Test func claimFallsBackToRemoteStoreOnNotFound() async throws {
        let (decorator, service, remoteStore) = makeDecorator()

        let data = try Data.randomOrError(of: 100)
        let hash = try data.blake2b32()
        service.claimErrorsByHash[hash] = notFoundError()
        remoteStore.storage[hash] = data

        let claimed = try await decorator.claimData(by: hash, recipient: MockRecipientProofProvider())

        #expect(claimed == data)
        #expect(remoteStore.requestedHashes == [hash])
    }

    @Test func claimReturnsNilWhenRemoteStoreMisses() async throws {
        let (decorator, service, _) = makeDecorator()

        let hash = try Data.randomOrError(of: 32)
        service.claimErrorsByHash[hash] = notFoundError()

        let claimed = try await decorator.claimData(by: hash, recipient: MockRecipientProofProvider())

        #expect(claimed == nil)
    }

    @Test func claimRethrowsOtherPoolErrors() async throws {
        let (decorator, service, remoteStore) = makeDecorator()

        let hash = try Data.randomOrError(of: 32)
        service.claimErrorsByHash[hash] = JSONRPCError(message: "internal", code: 1_000, data: nil)

        await #expect(throws: JSONRPCError.self) {
            try await decorator.claimData(by: hash, recipient: MockRecipientProofProvider())
        }

        #expect(remoteStore.requestedHashes.isEmpty)
    }

    @Test func ackSkippedForChainSourcedEntry() async throws {
        let (decorator, service, remoteStore) = makeDecorator()

        let data = try Data.randomOrError(of: 100)
        let hash = try data.blake2b32()
        service.claimErrorsByHash[hash] = notFoundError()
        remoteStore.storage[hash] = data

        _ = try await decorator.claimData(by: hash, recipient: MockRecipientProofProvider())
        try await decorator.acknowledgeReceivedData(by: hash, recipient: MockRecipientProofProvider())

        #expect(service.ackedHashes.isEmpty)
    }

    @Test func ackProxiedForPoolSourcedEntry() async throws {
        let (decorator, service, _) = makeDecorator()

        let data = try Data.randomOrError(of: 100)
        let hash = try data.blake2b32()
        service.storage[hash] = data

        _ = try await decorator.claimData(by: hash, recipient: MockRecipientProofProvider())
        try await decorator.acknowledgeReceivedData(by: hash, recipient: MockRecipientProofProvider())

        #expect(service.ackedHashes == [hash])
    }

    @Test func submitProxied() async throws {
        let (decorator, service, _) = makeDecorator()

        let data = try Data.randomOrError(of: 50)
        let submitted = try await decorator.submitData(
            data,
            from: NoProofProvider(),
            recipients: [.sr25519(Data(repeating: 1, count: 32))]
        )

        #expect(try submitted.hash == (data.blake2b32()))
        #expect(service.submitCallCount == 1)
    }
}

private extension HandoffServiceDecoratorTests {
    func makeDecorator() -> (HandoffServiceDecorator, MockHandoffService, MockLongTermRemoteStore) {
        let service = MockHandoffService()
        let remoteStore = MockLongTermRemoteStore()
        let decorator = HandoffServiceDecorator(handoffService: service, remoteStore: remoteStore)

        return (decorator, service, remoteStore)
    }

    func notFoundError() -> JSONRPCError {
        JSONRPCError(message: "Not found", code: HOPErrorCode.notFound, data: nil)
    }
}
