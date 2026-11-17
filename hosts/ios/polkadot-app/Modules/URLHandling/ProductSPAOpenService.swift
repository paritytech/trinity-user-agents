import Foundation
import Products
import func TrUAPIHost.appDeeplinkScheme

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
        guard var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return false }
        if let scheme = components.scheme, AppConfig.DeepLink.isKnownScheme(scheme) {
            components.scheme = appDeeplinkScheme()
        }
        guard let destination = components.url, let page = hostProvider.page(url: destination) else { return false }

        Task { @MainActor in
            moduleNavigator.openProduct(page: page)
        }
        return true
    }
}
