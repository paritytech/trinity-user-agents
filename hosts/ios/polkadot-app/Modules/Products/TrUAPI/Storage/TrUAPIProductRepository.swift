import Foundation
import Operation_iOS
import Products
import TrUAPIHost

final class TrUAPIProductRepository: DataProviderRepositoryProtocol {
    typealias Model = Product

    private func runtime() async throws -> TrUAPIHostRuntime {
        guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
            throw HostRejection.Rejected(reason: "Rust runtime provider unavailable")
        }
        return try await provider.sharedRuntime()
    }

    func fetchOperation(
        by modelIdClosure: @escaping () throws -> String,
        options: RepositoryFetchOptions
    ) -> BaseOperation<Product?> {
        AsyncTaskOperation {
            let identifier = try modelIdClosure()
            return try await self.runtime().products()
                .first { $0.productId == identifier }
                .map { Product(id: $0.productId, name: $0.name) }
        }
    }

    func fetchAllOperation(with options: RepositoryFetchOptions) -> BaseOperation<[Product]> {
        AsyncTaskOperation { try await self.runtime().products().map { Product(id: $0.productId, name: $0.name) } }
    }

    func fetchOperation(
        by request: RepositorySliceRequest,
        options: RepositoryFetchOptions
    ) -> BaseOperation<[Product]> {
        BaseOperation.createWithError(InMemoryDataProviderRepositoryError.unsupported)
    }

    func saveOperation(
        _ updateModelsBlock: @escaping () throws -> [Product],
        _ deleteIdsBlock: @escaping () throws -> [String]
    ) -> BaseOperation<Void> {
        AsyncTaskOperation {
            let runtime = try await self.runtime()
            let existing = try await runtime.products()
            for product in try updateModelsBlock() {
                let previous = existing.first { $0.productId == product.identifier }
                try await runtime.saveProduct(product: ProductRecord(
                    productId: product.identifier,
                    name: product.name,
                    iconCid: previous?.iconCid,
                    iconFormat: previous?.iconFormat,
                    workerUrlOverride: previous?.workerUrlOverride
                ))
            }
            for identifier in try deleteIdsBlock() { try await runtime.removeProduct(productId: identifier) }
            NotificationCenter.default.post(name: .truapiProductsChanged, object: nil)
        }
    }

    func replaceOperation(_ newModelsBlock: @escaping () throws -> [Product]) -> BaseOperation<Void> {
        AsyncTaskOperation {
            let products = try newModelsBlock()
            let runtime = try await self.runtime()
            let identifiers = Set(products.map(\.identifier))
            for existing in try await runtime.products() where !identifiers.contains(existing.productId) {
                try await runtime.removeProduct(productId: existing.productId)
            }
            for product in products {
                try await runtime.saveProduct(product: ProductRecord(
                    productId: product.identifier,
                    name: product.name,
                    iconCid: nil,
                    iconFormat: nil,
                    workerUrlOverride: nil
                ))
            }
            NotificationCenter.default.post(name: .truapiProductsChanged, object: nil)
        }
    }

    func fetchCountOperation() -> BaseOperation<Int> {
        AsyncTaskOperation { try await self.runtime().products().count }
    }

    func deleteAllOperation() -> BaseOperation<Void> {
        replaceOperation { [] }
    }
}

extension Notification.Name {
    static let truapiProductsChanged = Notification.Name("truapiProductsChanged")
}
