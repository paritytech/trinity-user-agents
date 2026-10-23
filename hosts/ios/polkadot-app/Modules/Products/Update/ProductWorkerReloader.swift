import Foundation
import Products

final class ProductWorkerReloader: ProductWorkerReloading {
    private let workerManager: ProductWorkerManaging
    private let chatExtensions: ChatExtensionsRegistering

    init(workerManager: ProductWorkerManaging, chatExtensions: ChatExtensionsRegistering) {
        self.workerManager = workerManager
        self.chatExtensions = chatExtensions
    }

    func reloadWorker(of productId: ProductId) async {
        await workerManager.reload(productId: productId)

        let extensionId = Product(id: productId, name: productId).extensionId

        guard let bot = chatExtensions.getChatExtensionBot(for: extensionId) as? ProductBot else { return }

        await bot.reload()
    }
}
