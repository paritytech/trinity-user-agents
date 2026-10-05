import Foundation
import UIKit
import Keystore_iOS
import Products
import ChainRegistry
import BulletinChain

/// Creates ``ProductBot`` instances for a given product.
///
/// The selected runtime owns the shared worker; bots only expose its chat surface.
final class ProductBotFactory {
    private let productFileProvider: ChatProductFileProviding
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let workerManager: ProductWorkerManaging
    private let logger: LoggerProtocol

    init(
        productFileProvider: ChatProductFileProviding,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        workerManager: ProductWorkerManaging,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productFileProvider = productFileProvider
        self.runtimeProvider = runtimeProvider
        self.workerManager = workerManager
        self.logger = logger
    }

    func createRustBot(product: Product) async throws -> ProductBot {
        let runtime = try await runtimeProvider.workerChatRuntime(productId: product.identifier)
        return ProductBot(product: product, runtime: runtime, logger: logger)
    }

    func create(resolved: ResolvedProduct) -> ProductBot? {
        guard workerSource(for: resolved) != nil else { return nil }

        let product = resolved.product

        let runtime = ManagedChatRuntime(productId: product.identifier, manager: workerManager)
        return ProductBot(product: product, runtime: runtime, logger: logger)
    }
}

enum ProductBotFactoryError: Error {
    case dependenciesUnavailable
}

private extension ProductBotFactory {
    /// A worker comes from a published manifest, or from a script installed by hand through debug
    /// settings. Nil means the product ships no chat surface and gets no bot.
    ///
    /// A bot is a chat surface, so a worker that declares `includes.chat: false` gets none — that
    /// is a valid background-only worker, and iOS has nothing else to run it on.
    func workerSource(for resolved: ResolvedProduct) -> ProductWorkerSource? {
        if let worker = resolved.executables.worker {
            guard worker.includesChat else { return nil }

            return ProductWorkerSource(contentId: worker.identifier, entryRelativePath: worker.entrypoint)
        }

        return productFileProvider.manualScriptEntryPath(productId: resolved.id).map {
            ProductWorkerSource(contentId: resolved.id, entryRelativePath: $0)
        }
    }

}
