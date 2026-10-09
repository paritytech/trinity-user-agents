import BigInt
import Coinage
import Foundation
import SubstrateSdk
import Testing
@testable import polkadot_app

/// Exercises the store through its real mappers — the full record mapper on save/fetch and the
/// write-only partial mapper on settle — against an in-memory Core Data stack.
struct IncomingPaymentCoreDataStoreTests {
    private let createdAt = Date(timeIntervalSince1970: 1_700_000_000)
    private let ownerId = Data(repeating: 7, count: 32)

    private func makeStore() -> IncomingPaymentCoreDataStore {
        IncomingPaymentCoreDataStore(storageFacade: UserDataStorageTestFacade())
    }

    private func payment(_ id: String, productId: String = "prod", amount: Balance = 100) -> IncomingPayment {
        IncomingPayment(paymentId: id, productId: productId, amount: amount, createdAt: createdAt,
                        outcome: nil, ownerId: ownerId)
    }

    @Test func saveThenFetchRoundTripsTheRecord() async throws {
        let store = makeStore()
        let big = Balance(UInt64.max) * Balance(UInt64.max)
        let saved = payment("p1", amount: big)

        try await store.save(saved, authorization: {})

        #expect(try await store.fetch(groupId: "top up:prod:p1") == saved)
        #expect(try await store.fetch(groupId: "top up:other:p1") == nil)
    }

    @Test func activePaymentsExcludeSettledOnes() async throws {
        let store = makeStore()
        try await store.save(payment("active"), authorization: {})
        try await store.save(payment("done"), authorization: {})

        try await store.settle(groupId: "top up:prod:done", ownerId: ownerId, outcome: .claimed, authorization: {})

        let active = try await store.fetchActivePayments()
        #expect(active.map(\.paymentId) == ["active"])
    }

    @Test func settleWritesOnlyTheVerdict() async throws {
        let store = makeStore()
        try await store.save(payment("p1", amount: 250), authorization: {})

        try await store.settle(groupId: "top up:prod:p1", ownerId: ownerId, outcome: .claimedPartially(actualClaimed: 40), authorization: {})

        let settled = try #require(try await store.fetch(groupId: "top up:prod:p1"))
        #expect(settled.outcome == .claimedPartially(actualClaimed: 40))
        #expect(settled.amount == 250)
        #expect(settled.createdAt == createdAt)
    }

    @Test func settlingAnUnknownRecordThrows() async throws {
        let store = makeStore()

        await #expect(throws: (any Error).self) {
            try await store.settle(groupId: "top up:prod:ghost", ownerId: ownerId, outcome: .notClaimed, authorization: {})
        }
    }

    @Test func revokedWalletCannotPersistIncomingCustody() async throws {
        enum Revoked: Error { case session }
        let store = makeStore()
        await #expect(throws: Revoked.session) {
            try await store.save(payment("stale"), authorization: { throw Revoked.session })
        }
        #expect(try await store.fetch(groupId: "top up:prod:stale") == nil)
        #expect(try await store.fetchActivePayments().isEmpty)
    }

    @Test func revokedWalletCannotSettleAnotherActivationPayment() async throws {
        enum Revoked: Error { case session }
        let store = makeStore()
        let value = payment("active")
        try await store.save(value, authorization: {})
        await #expect(throws: Revoked.session) {
            try await store.settle(groupId: value.groupId, ownerId: ownerId, outcome: .claimed,
                                   authorization: { throw Revoked.session })
        }
        #expect(try await store.fetch(groupId: value.groupId)?.outcome == nil)
        #expect(try await store.fetchActivePayments().map(\.paymentId) == ["active"])
    }

    @Test func unownedLegacyCustodyCannotBeAdoptedOrOverwritten() async throws {
        let facade = UserDataStorageTestFacade()
        let original = IncomingPaymentCoreDataStore(storageFacade: facade)
        let legacy = IncomingPayment(paymentId: "legacy", productId: "prod", amount: 500,
                                     createdAt: createdAt, outcome: nil, ownerId: nil)
        try await original.save(legacy, authorization: {})
        let restarted = IncomingPaymentCoreDataStore(storageFacade: facade)
        await #expect(throws: IncomingPaymentError.alreadyExists) {
            try await restarted.save(payment("legacy"), authorization: {})
        }
        await #expect(throws: IncomingPaymentError.notFound(legacy.groupId)) {
            try await restarted.settle(groupId: legacy.groupId, ownerId: ownerId,
                                       outcome: .claimed, authorization: {})
        }
        #expect(try await restarted.fetch(groupId: legacy.groupId) == legacy)
    }

    @Test(.timeLimit(.minutes(1)))
    func observeActivePaymentsReportsTheActiveSet() async throws {
        let store = makeStore()
        try await store.save(payment("active"), authorization: {})
        try await store.save(payment("done"), authorization: {})
        try await store.settle(groupId: "top up:prod:done", ownerId: ownerId, outcome: .claimed, authorization: {})

        for try await snapshot in store.observeActivePayments() {
            #expect(snapshot.map(\.paymentId) == ["active"])
            break
        }
    }
}
