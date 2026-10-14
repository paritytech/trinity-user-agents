import Foundation
import Products

final class AppDetailInteractor {
    weak var presenter: AppDetailInteractorOutputProtocol?

    private let productId: ProductId
    private let productResolver: ProductResolving
    private let logger: LoggerProtocol

    private var setupTask: Task<Void, Never>?

    init(
        productId: ProductId,
        productResolver: ProductResolving,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.productResolver = productResolver
        self.logger = logger
    }

    deinit {
        setupTask?.cancel()
    }
}

extension AppDetailInteractor: AppDetailInteractorInputProtocol {
    func setup() {
        setupTask = Task { [weak self] in
            guard let product = await self?.resolve() else { return }

            await self?.presenter?.didReceive(product: product)
        }
    }
}

private extension AppDetailInteractor {
    /// The screen that revokes the grant, so it has to render even for a product whose manifest
    /// is broken. It degrades to the domain; nothing launches from here.
    func resolve() async -> ResolvedProduct {
        do {
            return try await productResolver.resolve(productId)
        } catch {
            logger.error("Failed to resolve \(productId) for the app detail screen: \(error)")
            return .legacy(id: productId)
        }
    }
}
