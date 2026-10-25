import DurableTransactions
import Foundation
import Testing
@testable import Coinage

@Suite("Native transfer custody")
struct NativeTransferCustodyTests {
    private func custody(_ id: String, indices: [CoinageKeyIndex]) -> NativeTransferCustody {
        NativeTransferCustody(custodyId: id, entries: indices.map {
            .init(coinDerivationIndex: $0, valueExponent: 0, publicKey: testKey($0))
        })
    }

    private func register(
        _ registrations: [CoinageTxRegistration],
        custody: NativeTransferCustody,
        store: MockCoinageTxRepository
    ) async throws -> [CoinageTxId] {
        let ledger = store.ledger
        return try await DurableTxRegistrar(store: store.durable, owned: DurableTxOwnershipSet())
            .register(registrations.map(\.durable)) { scope, ids in
                try ledger.registerAssets(
                    registrations.map(\.assets), for: ids, custody: custody,
                    authorization: { try Task.checkCancellation() }, in: scope
                )
            }
    }

    @Test("Exact native custody survives relaunch while an ordinary provisional handoff is released")
    func exactMatchAndProvisionalRelease() async throws {
        let store = MockCoinageTxRepository()
        let retained = custody("exact", indices: [1])
        let provisional = OwnAsset.coin(2, testKey(2))
        try await store.ledger.retainNativeTransfer(retained, authorization: { try Task.checkCancellation() })
        try await store.precommitHandOff([provisional]) { _ in }

        try await store.releaseUncommittedHandoffs()

        #expect(store.durable.allEntries.isEmpty)
        #expect(store.handoffMarks == Set(retained.assets))
        #expect(try await store.ledger.retainedNativeTransfer(custodyId: "exact") == retained)
        await #expect(throws: CoinageTxError.inputHandedOff(testKey(1).toHex())) {
            try await store.register(.fixture(inputs: [.coin(.own(1, testKey(1)))]))
        }
        try await store.register(.fixture(inputs: [.coin(.own(2, testKey(2)))]))
    }

    @Test("An ordinary stale handoff cannot export a coin already retained by native custody")
    func staleOrdinaryHandoffIsRejected() async throws {
        let store = MockCoinageTxRepository()
        let retained = custody("native", indices: [1])
        try await store.ledger.retainNativeTransfer(retained, authorization: { try Task.checkCancellation() })
        let ordinary = MockCoinageTxService(store: store)
        await #expect(throws: CoinageTxError.handoffAlreadyReserved(testKey(1).toHex())) {
            try await ordinary.preCommitHandoff(retained.assets)
        }
        try await store.releaseUncommittedHandoffs()
        #expect(try await store.ledger.retainedNativeTransfer(custodyId: "native") == retained)
        #expect(store.handoffMarks == Set(retained.assets))
    }

    @Test("A claimed pass-through coin rejects the whole native batch before any durable registration")
    func rejectedCustodyIsSafeToRetry() async throws {
        let store = MockCoinageTxRepository()
        let claimed = CoinageTxEntry.fixture(inputs: [.coin(.own(1, testKey(1)))])
        try await store.register(claimed)
        let retained = custody("split", indices: [1, 3])
        let registration = CoinageTxRegistration.fixture(
            inputs: [.coin(.own(2, testKey(2)))], outputs: [.coin(3, testKey(3)), .coin(4, testKey(4))]
        )

        await #expect(throws: CoinageTxError.handoffOfClaimedAsset(testKey(1).toHex())) {
            try await register([registration], custody: retained, store: store)
        }
        #expect(store.durable.allEntries.map(\.id) == [claimed.id])
        #expect(store.handoffMarks.isEmpty)
        #expect(try await store.ledger.retainedNativeTransfer(custodyId: "split") == nil)

        try await store.updateStatus(claimed.id, to: .failure)
        _ = try await register([registration], custody: retained, store: store)
        #expect(try await store.ledger.retainedNativeTransfer(custodyId: "split") == retained)
    }

    @Test("Every unload transaction and all recipients commit together; replay cannot register again")
    func batchCustodyAndDuplicateIdentity() async throws {
        let store = MockCoinageTxRepository()
        let retained = custody("unload", indices: [1, 3, 5])
        let registrations: [CoinageTxRegistration] = [
            .fixture(inputs: [.recyclerVoucher(2, testKey(2))], outputs: [.coin(3, testKey(3)), .coin(4, testKey(4))]),
            .fixture(inputs: [.recyclerVoucher(6, testKey(6))], outputs: [.coin(5, testKey(5))])
        ]
        let ids = try await register(registrations, custody: retained, store: store)
        try await store.releaseUncommittedHandoffs()
        #expect(store.handoffMarks == Set(retained.assets))
        #expect(try await store.ledger.retainedNativeTransfer(custodyId: "unload") == retained)

        await #expect(throws: NativeTransferCustodyError.alreadyRegistered) {
            try await register(registrations, custody: retained, store: store)
        }
        #expect(store.durable.allEntries.map(\.id) == ids)
        #expect(!store.handoffMarks.contains(.coin(4, testKey(4))))
    }

    @Test("Engine asset validation rolls native custody back with a conflicting batch")
    func registrationInvariantRejectsCustody() async throws {
        let store = MockCoinageTxRepository()
        let retained = custody("conflicting", indices: [3])
        let registrations: [CoinageTxRegistration] = [
            .fixture(inputs: [.coin(.own(1, testKey(1)))], outputs: [.coin(3, testKey(3))]),
            .fixture(inputs: [.coin(.own(1, testKey(1)))], outputs: [.coin(4, testKey(4))])
        ]
        await #expect(throws: CoinageTxError.inputAlreadyClaimed(testKey(1).toHex())) {
            try await register(registrations, custody: retained, store: store)
        }
        #expect(store.durable.allEntries.isEmpty)
        #expect(store.handoffMarks.isEmpty)
        #expect(try await store.ledger.retainedNativeTransfer(custodyId: "conflicting") == nil)
    }

    @Test("A registered but incomplete asset record throws instead of authorizing a second spend")
    func partialRegistrationFailsClosed() async throws {
        let store = MockCoinageTxRepository()
        let retained = custody("partial", indices: [2])
        let ids = try await register([
            .fixture(inputs: [.coin(.own(1, testKey(1)))], outputs: [.coin(2, testKey(2))])
        ], custody: retained, store: store)
        store.ledger.removeAssets(for: try #require(ids.first))
        await #expect(throws: NativeTransferCustodyError.incompleteRegistration) {
            try await store.ledger.retainedNativeTransfer(custodyId: "partial")
        }
        #expect(store.durable.allEntries.map(\.id) == ids)
    }
}
