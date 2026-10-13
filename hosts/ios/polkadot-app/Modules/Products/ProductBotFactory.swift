import Foundation
import UIKit
import Keystore_iOS
import Products

/// Creates ``ProductBot`` instances for a given product.
///
/// The native path is a thin adapter over the shared ``ProductWorkerManager`` —
/// the worker itself is assembled by ``DefaultProductWorkerFactory``. The rust
/// path is a chat seam onto the product's one worker, which
/// ``TrUAPIWorkerManager`` runs.
final class ProductBotFactory {
    private let productFileProvider: ChatProductFileProviding
    private let settingsManager: SettingsManagerProtocol
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    private let workerManager: ProductWorkerManaging
    private let logger: LoggerProtocol

    init(
        productFileProvider: ChatProductFileProviding,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        workers: @Sendable @escaping () -> (any TrUAPIWorkerManaging)?,
        workerManager: ProductWorkerManaging,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productFileProvider = productFileProvider
        self.settingsManager = settingsManager
        self.runtimeProvider = runtimeProvider
        self.workers = workers
        self.workerManager = workerManager
        self.logger = logger
    }

    func create(resolved: ResolvedProduct) -> ProductBot? {
        guard servesChat(resolved) else { return nil }

        let product = resolved.product

        if settingsManager.isTrUAPIRuntimeEnabled, let workers = workers() {
            let runtime = TrUAPIChatHandler(
                productId: product.identifier,
                workers: workers,
                logger: logger
            )
            return ProductBot(product: product, runtime: runtime, logger: logger)
        }

        let runtime = ManagedChatRuntime(productId: product.identifier, manager: workerManager)
        return ProductBot(product: product, runtime: runtime, logger: logger)
    }
}

private extension ProductBotFactory {
    /// A bot is a chat surface, so a worker that declares `includes.chat: false`
    /// gets none. That is a valid background-only worker, and a bot for it
    /// would take a reference on a modality the product never declared.
    ///
    /// A script installed by hand through debug settings stands in for a product
    /// that has published no worker at all.
    func servesChat(_ resolved: ResolvedProduct) -> Bool {
        if ProductWorkerSource.published(for: resolved, serving: .chat) != nil { return true }

        return ProductWorkerSource.installedByHand(for: resolved) {
            productFileProvider.manualScriptEntryPath(productId: $0)
        } != nil
    }
}
