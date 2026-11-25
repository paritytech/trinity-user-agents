import Foundation
import FoundationExt
import UIKit
import Keystore_iOS
import PolkadotUI
import Products

/// Creates ``ProductBot`` instances for a given product.
///
/// The Rust chat handler shares the product's core-owned worker and presentation context.
final class ProductBotFactory {
    private let productFileProvider: ChatProductFileProviding
    private let settingsManager: SettingsManagerProtocol
    private let runtime: ProductWorkerRuntimeProvider
    private let contexts: ProductWorkerContexts
    private let workerManager: ProductWorkerManaging
    private let productImages: ProductImageSources
    private let logger: LoggerProtocol

    init(
        productFileProvider: ChatProductFileProviding,
        runtime: @escaping ProductWorkerRuntimeProvider,
        contexts: ProductWorkerContexts,
        workerManager: ProductWorkerManaging,
        productImages: ProductImageSources,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productFileProvider = productFileProvider
        self.settingsManager = settingsManager
        self.runtime = runtime
        self.contexts = contexts
        self.workerManager = workerManager
        self.productImages = productImages
        self.logger = logger
    }

    func create(resolved: ResolvedProduct) -> ProductBot? {
        guard let source = chatSource(resolved) else { return nil }

        let product = resolved.product
        let description = resolved.description?.nilIfEmpty
        let images = productImages.resolver(contentId: { source.contentId })
        let resolveImage = WidgetImageResolver.cached { await images.resolve($0) }

        if settingsManager.isTrUAPIRuntimeEnabled {
            let handler = TrUAPIChatHandler(
                productId: product.id,
                runtime: runtime,
                context: contexts.context(for: product.id),
                logger: logger
            )
            return ProductBot(
                product: product,
                description: description,
                runtime: handler,
                resolveImage: resolveImage,
                logger: logger
            )
        }

        let runtime = ManagedChatRuntime(productId: product.id, manager: workerManager)
        return ProductBot(
            product: product,
            description: description,
            runtime: runtime,
            resolveImage: resolveImage,
            logger: logger
        )
    }
}

private extension ProductBotFactory {
    /// A bot is a chat surface, so a worker that declares `includes.chat: false`
    /// gets none. That is a valid background-only worker, and a bot for it
    /// would take a reference on a modality the product never declared.
    ///
    /// A script installed by hand through debug settings stands in for a product
    /// that has published no worker at all.
    func chatSource(_ resolved: ResolvedProduct) -> ProductWorkerSource? {
        if let published = ProductWorkerSource.published(for: resolved, serving: .chat) { return published }

        return ProductWorkerSource.installedByHand(for: resolved) {
            productFileProvider.manualScriptEntryPath(productId: $0)
        }
    }
}
