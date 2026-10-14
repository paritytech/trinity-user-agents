import Foundation
import Operation_iOS
import Products

@Observable
final class DebugProductsViewModel {
    var products: [Product] = []
    var presentedSheet: DebugProductSheet?
    var downloadError: String?

    private let productRepository: AnyDataProviderRepository<Product>
    private let chatRepositoryFactory: ChatRepositoryMaking
    private let scriptDownloader: ProductScriptDownloaderProtocol
    private let scriptStorage: ChatScriptStorageProtocol
    private let notificationScheduler: ProductNotificationScheduling

    init(
        productRepository: AnyDataProviderRepository<Product>,
        chatRepositoryFactory: ChatRepositoryMaking,
        scriptDownloader: ProductScriptDownloaderProtocol = HTTPProductScriptDownloader(),
        scriptStorage: ChatScriptStorageProtocol = FileChatScriptStorage(),
        notificationScheduler: ProductNotificationScheduling = ProductNotificationScheduler.shared
    ) {
        self.productRepository = productRepository
        self.chatRepositoryFactory = chatRepositoryFactory
        self.scriptDownloader = scriptDownloader
        self.scriptStorage = scriptStorage
        self.notificationScheduler = notificationScheduler
    }

    func loadProducts() {
        Task { @MainActor in
            let fetched = try? await productRepository
                .fetchAllOperation(with: RepositoryFetchOptions())
                .asyncExecute()

            products = fetched ?? []
        }
    }

    func saveProduct(name: String, scriptURL: String) {
        let product = Product(id: UUID().uuidString, name: name)

        Task { @MainActor in
            downloadError = nil

            do {
                let content = try await scriptDownloader.download(url: scriptURL)
                try scriptStorage.saveScript(productId: product.identifier, content: content)
                try await productRepository.saveOperation({ [product] }, { [] }).asyncExecute()

                loadProducts()
            } catch {
                downloadError = "Script download failed: \(error.localizedDescription)"
            }
        }
    }

    func deleteProduct(_ product: Product) {
        let productChatRepository = chatRepositoryFactory.createRepository(
            forFilter: .roomChatsForExtension(product.extensionId)
        )

        Task { @MainActor [notificationScheduler] in
            try? await notificationScheduler.cancelAll(forProductId: product.identifier)
            try? await productRepository.saveOperation({ [] }, { [product.identifier] }).asyncExecute()
            try? await productChatRepository.deleteAllOperation().asyncExecute()
            try? scriptStorage.deleteScript(productId: product.identifier)
            loadProducts()
        }
    }

    func delete(at offsets: IndexSet) {
        for index in offsets {
            deleteProduct(products[index])
        }
    }
}

// MARK: - Sheet Model

struct DebugProductSheet: Swift.Identifiable {
    let id: String
    let name: String
    let scriptURL: String
    let isEditing: Bool

    static func add() -> DebugProductSheet {
        DebugProductSheet(id: "add", name: "", scriptURL: "", isEditing: false)
    }

    static func edit(_ product: Product) -> DebugProductSheet {
        DebugProductSheet(id: product.identifier, name: product.name, scriptURL: "", isEditing: true)
    }

    var title: String {
        isEditing ? "Edit Product" : "Add Product"
    }
}
