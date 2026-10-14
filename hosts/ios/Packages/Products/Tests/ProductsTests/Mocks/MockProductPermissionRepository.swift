import Foundation
@testable import Products

final class MockProductPermissionRepository: ProductPermissionRepositoryProtocol, @unchecked Sendable {
    // MARK: - State storage

    private var grants: [String: ProductPermissionGrant] = [:]
    private var oneTimeGrants: Set<String> = []

    // MARK: - Call tracking

    private(set) var grantCalls: [(productId: String, permission: ProductPermission)] = []
    private(set) var denyCalls: [(productId: String, permission: ProductPermission)] = []
    private(set) var grantOneTimeCalls: [(productId: String, permission: ProductPermission)] = []

    // MARK: - Stubbing

    func stubState(
        productId: String,
        permission: ProductPermission,
        state: ProductPermissionState
    ) {
        switch state {
        case .allowedAlways:
            grants[key(productId, permission)] = ProductPermissionGrant(
                productId: productId,
                permission: permission,
                granted: true,
                grantedAt: Date()
            )
        case .denied:
            grants[key(productId, permission)] = ProductPermissionGrant(
                productId: productId,
                permission: permission,
                granted: false,
                grantedAt: Date()
            )
        case .allowedOnce:
            oneTimeGrants.insert(key(productId, permission))
        case .notDetermined:
            grants.removeValue(forKey: key(productId, permission))
            oneTimeGrants.remove(key(productId, permission))
        }
    }

    // MARK: - Protocol

    func getPermissionState(
        productId: String,
        permission: ProductPermission
    ) async throws -> ProductPermissionState {
        let id = key(productId, permission)

        if oneTimeGrants.contains(id) {
            return .allowedOnce
        }

        guard let grant = grants[id] else {
            return .notDetermined
        }

        return grant.granted ? .allowedAlways : .denied
    }

    func isAnyAlwaysGranted(
        productId: String,
        typeName: String,
        keys: [String]
    ) async throws -> Bool {
        keys.contains { key in
            guard let permission = ProductPermission.from(typeName: typeName, key: key) else {
                return false
            }
            let id = self.key(productId, permission)
            return grants[id]?.granted == true
        }
    }

    func grant(productId: String, permission: ProductPermission) async throws {
        grantCalls.append((productId, permission))
        grants[key(productId, permission)] = ProductPermissionGrant(
            productId: productId,
            permission: permission,
            granted: true,
            grantedAt: Date()
        )
    }

    func deny(productId: String, permission: ProductPermission) async throws {
        denyCalls.append((productId, permission))
        let id = key(productId, permission)
        oneTimeGrants.remove(id)
        grants[id] = ProductPermissionGrant(
            productId: productId,
            permission: permission,
            granted: false,
            grantedAt: Date()
        )
    }

    func grantOneTime(productId: String, permission: ProductPermission) {
        grantOneTimeCalls.append((productId, permission))
        oneTimeGrants.insert(key(productId, permission))
    }

    func consumeOneTimeGrant(productId: String, permission: ProductPermission) -> Bool {
        oneTimeGrants.remove(key(productId, permission)) != nil
    }

    func revoke(productId: String, permission: ProductPermission) async throws {
        let id = key(productId, permission)
        grants.removeValue(forKey: id)
        oneTimeGrants.remove(id)
    }

    func revoke(productId: String, permissions: [ProductPermission]) async throws {
        for permission in permissions {
            let id = key(productId, permission)
            grants.removeValue(forKey: id)
            oneTimeGrants.remove(id)
        }
    }

    func revokeAllByProduct(productId: String) async throws {
        grants = grants.filter { $0.value.productId != productId }
        oneTimeGrants = oneTimeGrants.filter { !$0.hasPrefix("\(productId):") }
    }

    func getAllByProduct(productId: String) async throws -> [ProductPermissionGrant] {
        grants.values.filter { $0.productId == productId }
    }
}

private extension MockProductPermissionRepository {
    func key(_ productId: String, _ permission: ProductPermission) -> String {
        ProductPermissionGrant.makeIdentifier(productId: productId, permission: permission)
    }
}
