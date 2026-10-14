import Coinage
import CoreData
import Foundation
import Testing

@testable import polkadot_app
import DurableTransactions

/// The durability group-status seam against the real CoreData ledger over in-memory CoreData:
/// `getOperationGroupStatuses` / `subscribeOperationGroupStatuses` find every transaction registered
/// under `groupId = messageId`, which is how a claim or a payment tracks its group.
@Suite("Durability group-status query")
struct CoinageGroupStatusQueryTests {
    @Test("Fetch returns only the group's entries, in registration (sequence) order")
    func fetchByGroup() async throws {
        let store = makeStore()
        try await register(store, group: "msg-A", key: 1)
        try await register(store, group: "msg-B", key: 2)
        try await register(store, group: "msg-A", key: 3)

        let groupA = try await store.getOperationGroupStatuses("msg-A")
        #expect(groupA.count == 2)
        #expect(groupA.map(\.sequence) == groupA.map(\.sequence).sorted())
        #expect(Set(groupA.compactMap(\.groupId)) == ["msg-A"])

        let unknown = try await store.getOperationGroupStatuses("msg-Z")
        #expect(unknown.isEmpty)
    }

    @Test("Subscription's first emission is the group's current set")
    func subscribeCurrentSet() async throws {
        let store = makeStore()
        try await register(store, group: "g", key: 1)
        try await register(store, group: "g", key: 2)
        try await register(store, group: "other", key: 3)

        var iterator = store.subscribeOperationGroupStatuses("g").makeAsyncIterator()
        let first = try await iterator.next()

        #expect(first?.count == 2)
        #expect(first?.allSatisfy { $0.groupId == "g" } == true)
    }

    @Test("A group is scoped to the coinage domain")
    func groupScopedToDomain() async throws {
        let store = makeStore()
        try await register(store, group: "g", key: 1)

        // The same group id under another domain is a different group.
        _ = try await store.durable.register([
            DurableTxRegistration(
                domainId: TxDomainId("other"),
                groupId: "g",
                txHash: Data(repeating: 9, count: 32),
                checkpoint: BlockRef(number: 100, hash: Data([100])),
                mortalityBlocks: 64
            )
        ]) { _, _ in }

        let group = try await store.getOperationGroupStatuses("g")
        #expect(group.count == 1)
    }
}

/// The policy a transaction is registered with has to reach the store, or nothing can ever rebuild it.
///
/// A claim is the case that matters: it is built and submitted eagerly — registered, never scheduled —
/// and still carries a policy, because the peer's coin is money nothing else will collect. When the
/// policy was dropped at the persistence boundary the verdict writer read back `nil`, wrote the failure
/// as terminal, and the coin was stranded.
@Suite("Durable submission policy persistence")
struct DurableSubmissionPolicyPersistenceTests {
    private let policyId = SubmissionPolicyId("coinage-claim")

    @Test("a policy attached to an eager registration is read back")
    func registrationPolicyIsPersisted() async throws {
        let store = CoinageCoreDataLedger(storageFacade: UserDataStorageTestFacade())
        let params = Data([0xDE, 0xAD, 0xBE, 0xEF])

        let id = try #require(try await register(store, policy: SubmissionPolicy(id: policyId, params: params)))

        let stored = try await store.durable.getSubmissionPolicy(id: id)
        #expect(stored == SubmissionPolicy(id: policyId, params: params))
    }

    @Test("a registration with no policy reads back none, not an empty one")
    func registrationWithoutPolicyStaysNil() async throws {
        let store = CoinageCoreDataLedger(storageFacade: UserDataStorageTestFacade())

        let id = try #require(try await register(store, policy: nil))

        #expect(try await store.durable.getSubmissionPolicy(id: id) == nil)
    }

    @Test("a scheduled transaction's policy is read back too")
    func schedulePolicyIsPersisted() async throws {
        let store = CoinageCoreDataLedger(storageFacade: UserDataStorageTestFacade())
        let params = Data([1, 2, 3])

        let ids = try await store.durable.schedule(
            [DurableTxSchedule(
                domainId: .coinage,
                groupId: "group",
                policy: SubmissionPolicy(id: policyId, params: params)
            )],
            in: nil,
            onRegister: { _, _ in }
        )

        let id = try #require(ids.first)
        #expect(try await store.durable.getSubmissionPolicy(id: id) == SubmissionPolicy(id: policyId, params: params))
    }

    private func register(_ store: CoinageCoreDataLedger, policy: SubmissionPolicy?) async throws -> DurableTxId? {
        try await store.durable.register([
            DurableTxRegistration(
                domainId: .coinage,
                groupId: "group",
                txHash: Data(repeating: 0xAB, count: 32),
                checkpoint: BlockRef(number: 100, hash: Data([100])),
                mortalityBlocks: 64,
                policy: policy
            )
        ]) { _, _ in }.first
    }
}

private extension CoinageGroupStatusQueryTests {
    func makeStore() -> CoinageCoreDataLedger {
        CoinageCoreDataLedger(storageFacade: UserDataStorageTestFacade())
    }

    /// Registers a received-input entry (no local coin needed) under `group` — the seam under test is
    /// the persistence query, not the asset invariants.
    func register(_ store: CoinageCoreDataLedger, group: CoinageTxGroupId, key: UInt8) async throws {
        let registration = CoinageTxRegistration(
            txHash: Data(repeating: key, count: 32),
            checkpoint: BlockRef(number: 100, hash: Data([100])),
            mortalityBlocks: 64,
            groupId: group,
            inputs: [.coin(.received(Data(repeating: key, count: 32)))],
            outputs: []
        )
        try await store.register([registration])
    }
}
