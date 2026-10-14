import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing

struct HookRejected: Error {}

@Suite("Registration")
struct RegistrationTests {
    private let store = InMemoryDurableTxRepository()
    private let owned = DurableTxOwnershipSet()

    @Test("Ids come back in registration order and every one is owned by its submission")
    func idsInOrderAndOwned() async throws {
        let registrar = DurableTxRegistrar(store: store, owned: owned)

        let ids = try await registrar.register([.fixture(txHash: Data([1])), .fixture(txHash: Data([2]))]) { _, _ in }

        let entries = try await store.getAllEntries()
        #expect(entries.map(\.id) == ids)
        #expect(entries.map(\.txHash) == [Data([1]), Data([2])])
        #expect(ids.allSatisfy { owned.isOwned($0) })
    }

    @Test("The hook runs inside the transaction with the minted ids")
    func hookReceivesMintedIds() async throws {
        let registrar = DurableTxRegistrar(store: store, owned: owned)
        var seen: [DurableTxId] = []

        let ids = try await registrar.register([.fixture()]) { scope, hookIds in
            #expect(scope is InMemoryRegistrationScope)
            seen = hookIds
        }

        #expect(seen == ids)
    }

    @Test("A throwing hook rolls the whole batch back and takes no ownership")
    func throwingHookRollsBack() async throws {
        let registrar = DurableTxRegistrar(store: store, owned: owned)

        await #expect(throws: HookRejected.self) {
            try await registrar.register([.fixture(txHash: Data([1])), .fixture(txHash: Data([2]))]) { _, _ in
                throw HookRejected()
            }
        }

        let entries = try await store.getAllEntries()
        #expect(entries.isEmpty)
    }

    @Test("Sequence is monotonic across registrations")
    func sequenceMonotonic() async throws {
        let registrar = DurableTxRegistrar(store: store, owned: owned)

        _ = try await registrar.register([.fixture()]) { _, _ in }
        _ = try await registrar.register([.fixture()]) { _, _ in }
        _ = try await registrar.register([.fixture()]) { _, _ in }

        let all = try await store.getAllEntries()
        #expect(all.count == 3)
        #expect(all[0].sequence < all[1].sequence)
        #expect(all[1].sequence < all[2].sequence)
    }

    @Test("Registration checkpoint is preserved on the stored entry")
    func checkpointPreserved() async throws {
        let registrar = DurableTxRegistrar(store: store, owned: owned)

        let ids = try await registrar.register([.fixture(checkpoint: .fixture(150))]) { _, _ in }
        let entry = try await store.getEntry(id: #require(ids.first))

        #expect(entry?.checkpoint == .fixture(150))
    }

    @Test("Group entries are scoped to their domain")
    func groupScopedToDomain() async throws {
        let registrar = DurableTxRegistrar(store: store, owned: owned)

        _ = try await registrar.register([.fixture(domainId: TxDomainId("a"), groupId: "g")]) { _, _ in }
        _ = try await registrar.register([.fixture(domainId: TxDomainId("b"), groupId: "g")]) { _, _ in }

        let group = try await store.getGroupEntries(domain: TxDomainId("a"), groupId: "g")
        #expect(group.count == 1)
        #expect(group.first?.domainId == TxDomainId("a"))
    }
}
