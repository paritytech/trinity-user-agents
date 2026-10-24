import Foundation
import FoundationExt
import UIKit
import Keystore_iOS
import PolkadotUI
import Products

/// Creates ``ProductBot`` instances for a given product.
///
/// The native path is a thin adapter over the shared ``ProductWorkerManager`` —
/// the worker itself is assembled by ``DefaultProductWorkerFactory``. The rust
/// path is a chat seam onto the product's one worker, which
/// ``TrUAPIWorkerManager`` runs.
final class ProductBotFactory: @unchecked Sendable {
    private let contentLoader: ProductContentLoading
    private let productFileProvider: ChatProductFileProviding
    private let settingsManager: SettingsManagerProtocol
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    private let workerManager: ProductWorkerManaging
    private let productImages: ProductImageSources
    private let logger: LoggerProtocol

    init(
        contentLoader: ProductContentLoading,
        productFileProvider: ChatProductFileProviding,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        workers: @Sendable @escaping () -> (any TrUAPIWorkerManaging)?,
        workerManager: ProductWorkerManaging,
        productImages: ProductImageSources,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.contentLoader = contentLoader
        self.productFileProvider = productFileProvider
        self.settingsManager = settingsManager
        self.runtimeProvider = runtimeProvider
        self.workers = workers
        self.workerManager = workerManager
        self.productImages = productImages
        self.logger = logger
    }

    func create(resolved: ResolvedProduct) -> ProductBot? {
        guard let runtime = makeRuntime(for: resolved) else { return nil }

        let productId = resolved.id
        let reloadableRuntime = ReloadableChatRuntime(runtime: runtime) { [self] in
            let latest = try await contentLoader.open(productId, policy: .attempts(3)).resolved

            guard let runtime = makeRuntime(for: latest) else {
                throw ProductBotFactoryError.noChatWorker(productId)
            }

            return runtime
        }

        let images = productImages.resolver(contentId: { resolved.contentId(for: .worker) })

        return ProductBot(
            product: resolved.product,
            description: resolved.description?.nilIfEmpty,
            runtime: reloadableRuntime,
            resolveImage: WidgetImageResolver.cached { await images.resolve($0) },
            logger: logger
        )
    }
}

enum ProductBotFactoryError: Error {
    case noChatWorker(ProductId)
}

private extension ProductBotFactory {
    func makeRuntime(for resolved: ResolvedProduct) -> ChatRuntimeProtocol? {
        guard servesChat(resolved) else { return nil }

        let productId = resolved.product.identifier

        if settingsManager.isTrUAPIRuntimeEnabled, let workers = workers() {
            return TrUAPIChatHandler(productId: productId, workers: workers, logger: logger)
        }

        return ManagedChatRuntime(productId: productId, manager: workerManager)
    }

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
