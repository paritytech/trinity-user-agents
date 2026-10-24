import Foundation
import Products

final class ProductWorkerReloader: ProductWorkerReloading {
    private let workerManager: ProductWorkerManaging
    private let truapiWorkers: @Sendable () -> (any TrUAPIWorkerManaging)?
    private let chatExtensions: ChatExtensionsRegistering

    init(
        workerManager: ProductWorkerManaging,
        truapiWorkers: @escaping @Sendable () -> (any TrUAPIWorkerManaging)?,
        chatExtensions: ChatExtensionsRegistering
    ) {
        self.workerManager = workerManager
        self.truapiWorkers = truapiWorkers
        self.chatExtensions = chatExtensions
    }

    func reloadWorker(of productId: ProductId) async {
        await workerManager.reload(productId: productId)
        await truapiWorkers()?.reload(productId: productId)

        let extensionId = Product(id: productId, name: productId).extensionId

        guard let bot = chatExtensions.getChatExtensionBot(for: extensionId) as? ProductBot else { return }

        await bot.reload()
    }
}
