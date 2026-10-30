import Foundation
import Keystore_iOS
import Products

protocol ChatRuntimeMaking: Sendable {
    func makeRuntime(for resolved: ResolvedProduct) throws -> ChatRuntimeProtocol
}

enum ChatRuntimeFactoryError: Error {
    case noChatWorker(ProductId)
}

final class ChatRuntimeFactory: ChatRuntimeMaking, @unchecked Sendable {
    private let productFileProvider: ChatProductFileProviding
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    private let workerManager: ProductWorkerManaging
    private let settingsManager: SettingsManagerProtocol
    private let logger: LoggerProtocol

    init(
        productFileProvider: ChatProductFileProviding,
        workers: @Sendable @escaping () -> (any TrUAPIWorkerManaging)?,
        workerManager: ProductWorkerManaging,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productFileProvider = productFileProvider
        self.workers = workers
        self.workerManager = workerManager
        self.settingsManager = settingsManager
        self.logger = logger
    }

    func makeRuntime(for resolved: ResolvedProduct) throws -> ChatRuntimeProtocol {
        let productId = resolved.product.identifier

        guard servesChat(resolved) else {
            throw ChatRuntimeFactoryError.noChatWorker(productId)
        }

        if settingsManager.isTrUAPIRuntimeEnabled, let workers = workers() {
            return TrUAPIChatHandler(productId: productId, workers: workers, logger: logger)
        }

        return ManagedChatRuntime(productId: productId, manager: workerManager)
    }
}

private extension ChatRuntimeFactory {
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
