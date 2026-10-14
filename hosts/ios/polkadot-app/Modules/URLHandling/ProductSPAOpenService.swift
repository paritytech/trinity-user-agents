import Foundation
import Products

final class ProductSPAOpenService {
    private let moduleNavigator: ModuleNavigating
    private let hostProvider: ProductHostProviding

    init(
        moduleNavigator: ModuleNavigating,
        hostProvider: ProductHostProviding
    ) {
        self.moduleNavigator = moduleNavigator
        self.hostProvider = hostProvider
    }
}

extension ProductSPAOpenService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard let page = hostProvider.page(url: url) else {
            return false
        }

        Task { @MainActor in
            moduleNavigator.openProduct(page: page)
        }
        return true
    }
}
