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

    private func makeStore() -> IncomingPaymentCoreDataStore {
        IncomingPaymentCoreDataStore(storageFacade: UserDataStorageTestFacade())
    }

    private func payment(_ id: String, productId: String = "prod", amount: Balance = 100) -> IncomingPayment {
        IncomingPayment(paymentId: id, productId: productId, amount: amount, createdAt: createdAt, outcome: nil)
    }

    @Test func saveThenFetchRoundTripsTheRecord() async throws {
        let store = makeStore()
        let big = Balance(UInt64.max) * Balance(UInt64.max)
        let saved = payment("p1", amount: big)

        try await store.save(saved)

        #expect(try await store.fetch(groupId: "top up:prod:p1") == saved)
        #expect(try await store.fetch(groupId: "top up:other:p1") == nil)
    }

    @Test func activePaymentsExcludeSettledOnes() async throws {
        let store = makeStore()
        try await store.save(payment("active"))
        try await store.save(payment("done"))

        try await store.settle(groupId: "top up:prod:done", outcome: .claimed)

        let active = try await store.fetchActivePayments()
        #expect(active.map(\.paymentId) == ["active"])
    }

    @Test func settleWritesOnlyTheVerdict() async throws {
        let store = makeStore()
        try await store.save(payment("p1", amount: 250))

        try await store.settle(groupId: "top up:prod:p1", outcome: .claimedPartially(actualClaimed: 40))

        let settled = try #require(try await store.fetch(groupId: "top up:prod:p1"))
        #expect(settled.outcome == .claimedPartially(actualClaimed: 40))
        #expect(settled.amount == 250)
        #expect(settled.createdAt == createdAt)
    }

    @Test func settlingAnUnknownRecordThrows() async throws {
        let store = makeStore()

        await #expect(throws: (any Error).self) {
            try await store.settle(groupId: "top up:prod:ghost", outcome: .notClaimed)
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func observeActivePaymentsReportsTheActiveSet() async throws {
        let store = makeStore()
        try await store.save(payment("active"))
        try await store.save(payment("done"))
        try await store.settle(groupId: "top up:prod:done", outcome: .claimed)

        for try await snapshot in store.observeActivePayments() {
            #expect(snapshot.map(\.paymentId) == ["active"])
            break
        }
    }
}
