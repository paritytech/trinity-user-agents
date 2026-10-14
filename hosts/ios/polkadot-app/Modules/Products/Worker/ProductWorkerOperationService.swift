import Foundation
import os
import Operation_iOS
import AsyncExtensions
import Products

/// A persisted keep-alive operation opened by a product's worker.
struct ProductOperationRecord: Equatable {
    let productId: ProductId
    let id: UInt32
    let label: String?
    let startedAt: Date
}

extension ProductOperationRecord: Operation_iOS.Identifiable {
    /// Unique per product's open operation, and the primary key the repository
    /// stores and deletes by.
    var identifier: String { Self.identifier(productId: productId, id: id) }

    static func identifier(productId: ProductId, id: UInt32) -> String {
        "\(productId)-\(id)"
    }
}

/// Persistence for open worker operations. The persisted set is the source of
/// truth for worker keep-alive: ``ProductWorkerFacade`` subscribes to it and
/// holds a worker lock per operation.
protocol ProductOperationStoring: Sendable {
    func save(_ record: ProductOperationRecord) async throws
    func delete(productId: ProductId, id: UInt32) async throws
    func all() async throws -> [ProductOperationRecord]
    func clearAll() async throws
    /// Emits the current open operations, then again on every change.
    func subscribe() -> AnyAsyncSequence<[ProductOperationRecord]>
}

/// Opens and closes worker keep-alive operations. Opening one persists a record;
/// ``ProductWorkerFacade`` observes it and starts the worker, so an open
/// operation keeps the product's worker alive. Closing removes the record.
protocol ProductWorkerOperating: Sendable {
    func beginOperation(productId: ProductId, label: String?) async throws -> UInt32
    func endOperation(productId: ProductId, id: UInt32) async throws
}

final class ProductWorkerOperationService: ProductWorkerOperating, @unchecked Sendable {
    private let store: ProductOperationStoring

    init(store: ProductOperationStoring) {
        self.store = store
    }

    func beginOperation(productId: ProductId, label: String?) async throws -> UInt32 {
        // Random and unique among this product's open operations. 0 is avoided so
        // it never reads as an unset id. The facade's reconciliation observes the
        // saved record and starts the worker.
        let existing = try await Set(store.all().filter { $0.productId == productId }.map(\.id))
        var id = UInt32.random(in: 1 ... .max)
        while existing.contains(id) {
            id = UInt32.random(in: 1 ... .max)
        }

        try await store.save(
            ProductOperationRecord(productId: productId, id: id, label: label, startedAt: Date())
        )

        return id
    }

    func endOperation(productId: ProductId, id: UInt32) async throws {
        // Idempotent: deleting an unknown or already-ended id is a no-op. The
        // facade releases the worker when the record disappears.
        try await store.delete(productId: productId, id: id)
    }
}
