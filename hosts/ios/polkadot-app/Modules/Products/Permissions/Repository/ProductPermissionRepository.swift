import Foundation
import Operation_iOS
import Products

final class ProductPermissionRepository: @unchecked Sendable {
    private let storageFacade: StorageFacadeProtocol
    private let mapper: AnyCoreDataMapper<ProductPermissionGrant, CDProductPermissionGrant>
    private let repository: AnyDataProviderRepository<ProductPermissionGrant>

    private let oneTimeLock = NSLock()
    private var oneTimeGrants: Set<String> = []

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade

        let mapper = AnyCoreDataMapper(ProductPermissionGrantMapper())
        self.mapper = mapper

        repository = AnyDataProviderRepository(
            storageFacade.createRepository(mapper: mapper)
        )
    }
}

extension ProductPermissionRepository: ProductPermissionRepositoryProtocol {
    // MARK: - Persistent grants

    func getPermissionState(
        productId: String,
        permission: ProductPermission
    ) async throws -> ProductPermissionState {
        let identifier = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        guard !hasOneTimeGrant(for: identifier) else {
            return .allowedOnce
        }

        let grant = try await repository.fetchOperation(
            by: { identifier },
            options: .init()
        )
        .asyncExecute()

        guard let grant else {
            return .notDetermined
        }

        return grant.granted ? .allowedAlways : .denied
    }

    func isAnyAlwaysGranted(
        productId: String,
        typeName: String,
        keys: [String]
    ) async throws -> Bool {
        let identifiers = keys.compactMap { key -> String? in
            guard let permission = ProductPermission.from(typeName: typeName, key: key) else {
                return nil
            }
            return ProductPermissionGrant.makeIdentifier(
                productId: productId,
                permission: permission
            )
        }

        guard !identifiers.isEmpty else { return false }

        let grants = try await productRepository(for: productId)
            .fetchAllOperation(with: .init())
            .asyncExecute()

        return grants.contains { $0.granted && identifiers.contains($0.identifier) }
    }

    func grant(productId: String, permission: ProductPermission) async throws {
        let grant = ProductPermissionGrant(
            productId: productId,
            permission: permission,
            granted: true,
            grantedAt: Date()
        )

        try await repository.saveOperation({ [grant] }, { [] }).asyncExecute()
    }

    func deny(productId: String, permission: ProductPermission) async throws {
        let grant = ProductPermissionGrant(
            productId: productId,
            permission: permission,
            granted: false,
            grantedAt: Date()
        )

        clearOneTimeGrant(for: grant.identifier)

        try await repository.saveOperation({ [grant] }, { [] }).asyncExecute()
    }

    func revoke(productId: String, permission: ProductPermission) async throws {
        let identifier = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        clearOneTimeGrant(for: identifier)

        try await repository.saveOperation({ [] }, { [identifier] }).asyncExecute()
    }

    func revoke(productId: String, permissions: [ProductPermission]) async throws {
        guard !permissions.isEmpty else { return }

        let identifiers = permissions.map {
            ProductPermissionGrant.makeIdentifier(productId: productId, permission: $0)
        }

        for identifier in identifiers {
            clearOneTimeGrant(for: identifier)
        }

        try await repository.saveOperation({ [] }, { identifiers }).asyncExecute()
    }

    func revokeAllByProduct(productId: String) async throws {
        let allGrants = try await getAllByProduct(productId: productId)
        let identifiers = allGrants.map(\.identifier)

        guard !identifiers.isEmpty else { return }

        for identifier in identifiers {
            clearOneTimeGrant(for: identifier)
        }

        try await repository.saveOperation({ [] }, { identifiers }).asyncExecute()
    }

    func getAllByProduct(productId: String) async throws -> [ProductPermissionGrant] {
        try await productRepository(for: productId)
            .fetchAllOperation(with: .init())
            .asyncExecute()
    }

    // MARK: - One-time grants

    func grantOneTime(productId: String, permission: ProductPermission) {
        let key = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }
        oneTimeGrants.insert(key)
    }

    func consumeOneTimeGrant(productId: String, permission: ProductPermission) -> Bool {
        let key = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }
        return oneTimeGrants.remove(key) != nil
    }
}

private extension ProductPermissionRepository {
    func hasOneTimeGrant(for key: String) -> Bool {
        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }

        return oneTimeGrants.contains(key)
    }

    func clearOneTimeGrant(for key: String) {
        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }

        oneTimeGrants.remove(key)
    }

    func productRepository(
        for productId: String
    ) -> AnyDataProviderRepository<ProductPermissionGrant> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: .permissionGrant(productId: productId),
                sortDescriptors: [],
                mapper: mapper
            )
        )
    }
}
