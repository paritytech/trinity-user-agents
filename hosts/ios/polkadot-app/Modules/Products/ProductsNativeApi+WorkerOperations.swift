import Foundation

// MARK: - Worker Operations

extension ProductsNativeApi {
    func workerBeginOperation(label: String?) async throws -> UInt32 {
        try await workerOperations.beginOperation(productId: productId, label: label)
    }

    func workerEndOperation(id: UInt32) async throws {
        try await workerOperations.endOperation(productId: productId, id: id)
    }
}
