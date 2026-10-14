import Testing
import Foundation
@testable import polkadot_app

@Suite("CoreDataProductOperationStore")
struct CoreDataProductOperationStoreTests {
    private func makeStore() -> CoreDataProductOperationStore {
        CoreDataProductOperationStore(storageFacade: UserDataStorageTestFacade())
    }

    private let startedAt = Date(timeIntervalSince1970: 1_700_000_000)

    @Test("save then fetch round-trips every field, including a nil label")
    func saveRoundTrips() async throws {
        let store = makeStore()

        let labelled = ProductOperationRecord(productId: "getcash", id: 7, label: "sync", startedAt: startedAt)
        let unlabelled = ProductOperationRecord(productId: "getcash", id: 8, label: nil, startedAt: startedAt)

        try await store.save(labelled)
        try await store.save(unlabelled)

        let all = try await store.all().sorted { $0.id < $1.id }
        #expect(all == [labelled, unlabelled])
    }

    @Test("delete removes only the matching operation")
    func deleteRemovesOne() async throws {
        let store = makeStore()

        try await store.save(ProductOperationRecord(productId: "getcash", id: 1, label: nil, startedAt: startedAt))
        try await store.save(ProductOperationRecord(productId: "getcash", id: 2, label: nil, startedAt: startedAt))

        try await store.delete(productId: "getcash", id: 1)

        let remaining = try await store.all()
        #expect(remaining.map(\.id) == [2])
    }

    @Test("deleting an unknown operation is a no-op")
    func deleteUnknownIsNoOp() async throws {
        let store = makeStore()
        try await store.save(ProductOperationRecord(productId: "getcash", id: 1, label: nil, startedAt: startedAt))

        try await store.delete(productId: "getcash", id: 999)
        try await store.delete(productId: "other", id: 1)

        let remaining = try await store.all()
        #expect(remaining.map(\.id) == [1])
    }

    @Test("clearAll empties the table")
    func clearAllEmpties() async throws {
        let store = makeStore()
        try await store.save(ProductOperationRecord(productId: "getcash", id: 1, label: nil, startedAt: startedAt))
        try await store.save(ProductOperationRecord(productId: "other", id: 2, label: "x", startedAt: startedAt))

        try await store.clearAll()

        let all = try await store.all()
        #expect(all.isEmpty)
    }
}
