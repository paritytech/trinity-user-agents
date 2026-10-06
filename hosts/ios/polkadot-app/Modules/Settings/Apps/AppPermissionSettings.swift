import Foundation
import AsyncExtensions
import Keystore_iOS
import Products
import TrUAPIHost

enum AppPermissionRecord: Equatable {
    case legacy(ProductPermissionGrant)
    case rust(PermissionRecord)

    var productId: ProductId {
        switch self {
        case let .legacy(grant): grant.productId
        case let .rust(record): record.productId
        }
    }

    func hasSameIdentity(as other: AppPermissionRecord) -> Bool {
        switch (self, other) {
        case let (.legacy(lhs), .legacy(rhs)): lhs.identifier == rhs.identifier
        case let (.rust(lhs), .rust(rhs)): lhs.productId == rhs.productId && lhs.request == rhs.request
        default: false
        }
    }

    var isNotifications: Bool {
        switch self {
        case let .legacy(grant): grant.permission == .deviceCapability(.notifications)
        case let .rust(record): record.request == .device(.notifications)
        }
    }
}

enum AppPermissionSettings {
    case legacy(ProductPermissionDataProviderMaking, ProductPermissionRepositoryProtocol)
    case rust(TrUAPIHostRuntimeProviding)

    init(
        runtimeEnabled: Bool = SettingsManager.shared.isTrUAPIRuntimeEnabled,
        runtimeProvider: TrUAPIHostRuntimeProviding? = RootDependencyLocator.getDependency()
    ) throws {
        if runtimeEnabled {
            guard let runtimeProvider else {
                throw HostRejection.Rejected(reason: "TrUAPI runtime provider unavailable")
            }
            self = .rust(runtimeProvider)
        } else {
            self = .legacy(ProductPermissionDataProviderFactory(), ProductPermissionRepository())
        }
    }

    func grantedRecords(productId: ProductId? = nil) async throws -> AnyAsyncSequence<[AppPermissionRecord]> {
        switch self {
        case let .legacy(factory, _):
            let grants = productId.map { factory.subscribeGrants(productId: $0) }
                ?? factory.subscribeAllGrants(grantedOnly: true)
            return grants.map { $0.map(AppPermissionRecord.legacy) }.eraseToAnyAsyncSequence()
        case let .rust(provider):
            let runtime = try await provider.constructedRuntime()
            return runtime.permissionRecords(productId: productId)
                .map { $0.filter { $0.status == .authorized }.map(AppPermissionRecord.rust) }
                .eraseToAnyAsyncSequence()
        }
    }

    func revoke(_ records: [AppPermissionRecord]) async throws {
        switch self {
        case let .legacy(_, repository):
            let grants = try records.map { record in
                guard case let .legacy(grant) = record else {
                    throw HostRejection.Rejected(reason: "Permission belongs to another settings store")
                }
                return grant
            }
            for (productId, grants) in Dictionary(grouping: grants, by: \.productId) {
                try await repository.revoke(productId: productId, permissions: grants.map(\.permission))
            }
        case let .rust(provider):
            let runtime = try await provider.constructedRuntime()
            for record in records {
                guard case let .rust(record) = record else {
                    throw HostRejection.Rejected(reason: "Permission belongs to another settings store")
                }
                try await runtime.setPermissionRecord(
                    productId: record.productId,
                    request: record.request,
                    status: .notDetermined
                )
            }
        }
    }
}
