import Foundation
import Products

final class AppsListInteractor {
    weak var presenter: AppsListInteractorOutputProtocol?

    private let providerFactory: ProductPermissionDataProviderMaking
    private let productResolver: ProductResolving
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?
    private var resolvedById: [ProductId: ResolvedProduct] = [:]

    init(
        providerFactory: ProductPermissionDataProviderMaking,
        productResolver: ProductResolving,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.providerFactory = providerFactory
        self.productResolver = productResolver
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
    }
}

extension AppsListInteractor: AppsListInteractorInputProtocol {
    func setup() {
        subscriptionTask = Task { [weak self, providerFactory, logger] in
            let stream = providerFactory.subscribeAllGrants(grantedOnly: true)

            do {
                for try await grants in stream {
                    let productIds = Self.distinctProductIds(from: grants)
                    await self?.handle(productIds: productIds)
                }
            } catch {
                logger.error("Apps list subscription error: \(error)")
            }
        }
    }
}

private extension AppsListInteractor {
    static func distinctProductIds(from grants: [ProductPermissionGrant]) -> [ProductId] {
        var seen = Set<ProductId>()
        return grants.compactMap { grant in
            seen.insert(grant.productId).inserted ? grant.productId : nil
        }
    }

    func handle(productIds: [ProductId]) async {
        // Rows first, off the chain entirely: the user granted these products, so the list is
        // known before any manifest is. Names fill in behind it, and stay filled in when a later
        // grant change re-emits the same products.
        await presenter?.didReceive(products: productIds.map { resolvedById[$0] ?? .legacy(id: $0) })

        let products = await resolve(productIds)
        resolvedById = products.reduce(into: [:]) { $0[$1.id] = $1 }

        await presenter?.didReceive(products: products)
    }

    /// Every granted product belongs in this list, including one whose manifest is broken:
    /// dropping the row would leave the grant standing with nothing left to revoke it from.
    /// Nothing launches from here, so a row that shows only a domain costs nothing.
    func resolve(_ productIds: [ProductId]) async -> [ResolvedProduct] {
        // Indexed so the concurrent results keep the order the grants arrived in.
        await withTaskGroup(of: (Int, ResolvedProduct).self) { group in
            for (index, productId) in productIds.enumerated() {
                group.addTask { [productResolver, logger] in
                    do {
                        return try await (index, productResolver.resolve(productId))
                    } catch {
                        logger.error("Failed to resolve \(productId) for the apps list: \(error)")
                        return (index, .legacy(id: productId))
                    }
                }
            }

            return await group
                .reduce(into: [(Int, ResolvedProduct)]()) { $0.append($1) }
                .sorted { $0.0 < $1.0 }
                .map(\.1)
        }
    }
}
