import Testing
import Foundation
import os
import AsyncExtensions
import Products
@testable import polkadot_app

private final class FakeOperationStore: ProductOperationStoring, @unchecked Sendable {
    private let records = OSAllocatedUnfairLock(initialState: [ProductOperationRecord]())

    func save(_ record: ProductOperationRecord) async throws {
        records.withLock { $0.append(record) }
    }

    func delete(productId: ProductId, id: UInt32) async throws {
        records.withLock { $0.removeAll { $0.productId == productId && $0.id == id } }
    }

    func all() async throws -> [ProductOperationRecord] { records.withLock { $0 } }
    func clearAll() async throws { records.withLock { $0 = [] } }
    func subscribe() -> AnyAsyncSequence<[ProductOperationRecord]> {
        AsyncStream<[ProductOperationRecord]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    var count: Int { records.withLock { $0.count } }
    var isEmpty: Bool { records.withLock { $0.isEmpty } }
}

@Suite("ProductWorkerOperationService")
struct ProductWorkerOperationServiceTests {
    @Test("begin assigns distinct non-zero ids and persists")
    func beginPersistsDistinctIds() async throws {
        let store = FakeOperationStore()
        let service = ProductWorkerOperationService(store: store)

        let first = try await service.beginOperation(productId: "getcash", label: "sync")
        let second = try await service.beginOperation(productId: "getcash", label: nil)

        #expect(first != 0)
        #expect(second != 0)
        #expect(first != second)
        #expect(store.count == 2)
    }

    @Test("end removes the record and is idempotent")
    func endDeletesAndIsIdempotent() async throws {
        let store = FakeOperationStore()
        let service = ProductWorkerOperationService(store: store)

        let id = try await service.beginOperation(productId: "getcash", label: nil)
        try await service.endOperation(productId: "getcash", id: id)
        #expect(store.isEmpty)

        // Ending again, or an unknown id, is a no-op success.
        try await service.endOperation(productId: "getcash", id: id)
        try await service.endOperation(productId: "getcash", id: 999)
        #expect(store.isEmpty)
    }
}
