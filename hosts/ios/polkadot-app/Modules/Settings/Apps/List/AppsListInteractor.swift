import Foundation
import Products

final class AppsListInteractor {
    weak var presenter: AppsListInteractorOutputProtocol?

    private let providerFactory: ProductPermissionDataProviderMaking
    private let coreAuthorizations: TrUAPIPermissionAuthorizing?
    private let productResolver: ProductResolving
    private let logger: LoggerProtocol

    private var nativeGrantsTask: Task<Void, Never>?
    private var coreGrantsTask: Task<Void, Never>?

    @MainActor private var nativeProductIds: [ProductId] = []
    @MainActor private var coreProductIds: [ProductId] = []
    @MainActor private var resolvedById: [ProductId: ResolvedProduct] = [:]
    @MainActor private var publishGeneration = 0

    init(
        providerFactory: ProductPermissionDataProviderMaking,
        coreAuthorizations: TrUAPIPermissionAuthorizing?,
        productResolver: ProductResolving,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.providerFactory = providerFactory
        self.coreAuthorizations = coreAuthorizations
        self.productResolver = productResolver
        self.logger = logger
    }

    deinit {
        nativeGrantsTask?.cancel()
        coreGrantsTask?.cancel()
    }
}

extension AppsListInteractor: AppsListInteractorInputProtocol {
    func setup() {
        nativeGrantsTask = Task { [weak self, providerFactory, logger] in
            do {
                for try await grants in providerFactory.subscribeAllGrants(grantedOnly: true) {
                    await self?.handle(nativeProductIds: Self.distinctProductIds(from: grants))
                }
            } catch {
                logger.error("Apps list subscription error: \(error)")
            }
        }

        coreGrantsTask = Task { [weak self, logger] in
            // Subscribed before the first read, so a change raised during it is
            // buffered rather than lost.
            guard var changes = self?.coreAuthorizations?.changes.makeAsyncIterator() else { return }

            await self?.reloadCoreGrants()

            do {
                while try await changes.next() != nil {
                    await self?.reloadCoreGrants()
                }
            } catch {
                logger.error("Core permission changes subscription error: \(error)")
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

    @MainActor
    func reloadCoreGrants() async {
        guard let coreAuthorizations else { return }

        do {
            let grants = try await coreAuthorizations.grants()
            await handle(coreProductIds: Self.distinctProductIds(from: grants))
        } catch {
            logger.error("Failed to list the core's permission grants: \(error)")
        }
    }

    @MainActor
    func handle(nativeProductIds: [ProductId]) async {
        self.nativeProductIds = nativeProductIds
        await publish()
    }

    @MainActor
    func handle(coreProductIds: [ProductId]) async {
        self.coreProductIds = coreProductIds
        await publish()
    }

    /// One row per product whichever store holds its grants. The native store
    /// lists in name order; the core lists in storage order, so its products
    /// are sorted before they follow.
    @MainActor
    func publish() async {
        var seen = Set<ProductId>()
        let coreOnly = coreProductIds.sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
        let productIds = (nativeProductIds + coreOnly).filter { seen.insert($0).inserted }

        await handle(productIds: productIds)
    }

    @MainActor
    func handle(productIds: [ProductId]) async {
        publishGeneration += 1
        let generation = publishGeneration

        // Rows first, off the chain entirely: the user granted these products, so the list is
        // known before any manifest is. Names fill in behind it, and stay filled in when a later
        // grant change re-emits the same products.
        presenter?.didReceive(products: productIds.map { resolvedById[$0] ?? .legacy(id: $0) })

        let products = await resolve(productIds)

        guard generation == publishGeneration else { return }

        resolvedById = products.reduce(into: [:]) { $0[$1.id] = $1 }

        presenter?.didReceive(products: products)
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
