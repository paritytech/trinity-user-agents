import Foundation
import Operation_iOS
import Products
import SubstrateSdk
import TrUAPIHost

protocol ProductPermissionAuthority: Sendable {
    func permissionAuthorizationProducts() async throws -> [String]
    func permissionAuthorizations(productId: String) async throws -> [PermissionAuthorizationEntry]
    func importPermissionAuthorizations(
        productId: String,
        entries: [PermissionAuthorizationEntry]
    ) async throws -> [PermissionAuthorizationEntry]
    func setPermissionAuthorizationStatus(
        productId: String,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus
    ) async throws
    func permissionAuthorizationRevision(productId: String) throws -> UInt64
    func setPermissionAuthorizationStatusIfCurrent(
        productId: String,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
        revision: UInt64
    ) async throws -> Bool
}

extension TrUAPIHostRuntime: ProductPermissionAuthority {}

extension Notification.Name {
    static let productPermissionAuthorizationsChanged = Notification.Name("ProductPermissionAuthorizationsChanged")
}

final class ProductPermissionRepository: @unchecked Sendable {
    private let storageFacade: StorageFacadeProtocol
    private let mapper: AnyCoreDataMapper<ProductPermissionGrant, CDProductPermissionGrant>
    private let repository: AnyDataProviderRepository<ProductPermissionGrant>

    private let oneTimeLock = NSLock()
    private var oneTimeGrants: [String: UInt64] = [:]
    private let authority: @Sendable () throws -> any ProductPermissionAuthority

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        authority: @escaping @Sendable () throws -> any ProductPermissionAuthority = {
            guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
                throw ProductPermissionMappingError.runtimeUnavailable
            }
            return try provider.sharedRuntime()
        }
    ) {
        self.authority = authority
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
        if let request = try permission.authorizationRequest() {
            let legacy = try await legacyGrants(productId: productId)
            let entries = try await canonicalEntries(productId: productId, legacy: legacy)
            let status = entries.first { $0.request == request }?.status ?? .notDetermined
            if status == .denied { return .denied }
            if hasOneTimeGrant(
                for: ProductPermissionGrant.makeIdentifier(productId: productId, permission: permission),
                productId: productId
            ) { return .allowedOnce }
            return status == .authorized ? .allowedAlways : .notDetermined
        }
        let identifier = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        guard !hasOneTimeGrant(for: identifier, productId: productId) else {
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
        let grants = try await getAllByProduct(productId: productId)
        let permissions = try keys.map { key -> ProductPermission in
            guard let permission = ProductPermission.from(typeName: typeName, key: key) else {
                throw ProductPermissionMappingError.unsupported(typeName, key)
            }
            return try permission.canonicalPermission()
        }
        return grants.contains { $0.granted && permissions.contains($0.permission) }
    }

    func grant(productId: String, permission: ProductPermission) async throws {
        if let request = try permission.authorizationRequest() {
            try await authority().setPermissionAuthorizationStatus(
                productId: productId, request: request, status: .authorized
            )
            return
        }
        let grant = ProductPermissionGrant(
            productId: productId,
            permission: permission,
            granted: true,
            grantedAt: Date()
        )

        try await repository.saveOperation({ [grant] }, { [] }).asyncExecute()
    }

    func deny(productId: String, permission: ProductPermission) async throws {
        if let request = try permission.authorizationRequest() {
            try await authority().setPermissionAuthorizationStatus(
                productId: productId, request: request, status: .denied
            )
            return
        }
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
        if let request = try permission.authorizationRequest() {
            // The core persists an explicit reset, so import cannot restore an old grant.
            try await authority().setPermissionAuthorizationStatus(
                productId: productId, request: request, status: .notDetermined
            )
            return
        }
        let identifier = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        clearOneTimeGrant(for: identifier)

        try await repository.saveOperation({ [] }, { [identifier] }).asyncExecute()
    }

    func revoke(productId: String, permissions: [ProductPermission]) async throws {
        for permission in permissions {
            try await revoke(productId: productId, permission: permission)
        }
    }

    func revokeAllByProduct(productId: String) async throws {
        let grants = try await getAllByProduct(productId: productId)
        try await revoke(productId: productId, permissions: grants.map(\.permission))
    }

    func settingsGrants(legacy: [ProductPermissionGrant]) async throws -> [ProductPermissionGrant] {
        let nativeIds = try await authority().permissionAuthorizationProducts()
        var productIds = Set(legacy.map(\.productId))
        productIds.formUnion(nativeIds.filter { $0.contains(".") })
        for productId in nativeIds where !productId.contains(".") {
            if !productIds.contains(where: { ProductPermission.bareProductLabel($0) == productId }) {
                productIds.insert(productId)
            }
        }
        var grants: [ProductPermissionGrant] = []
        for productId in productIds.sorted() {
            grants.append(contentsOf: try await getAllByProduct(productId: productId))
        }
        return grants
    }

    func getAllByProduct(productId: String) async throws -> [ProductPermissionGrant] {
        let legacy = try await legacyGrants(productId: productId)
        let entries = try await canonicalEntries(productId: productId, legacy: legacy)
        var grants = legacy.filter { $0.permission == .balanceAccess }
        for entry in entries where entry.status != .notDetermined {
            for permission in try ProductPermission.fromAuthorization(entry.request) {
                grants.append(ProductPermissionGrant(
                    productId: productId,
                    permission: permission,
                    granted: entry.status == .authorized,
                    grantedAt: nil
                ))
            }
        }
        return grants
    }

    // MARK: - One-time grants

    func grantOneTime(productId: String, permission: ProductPermission) {
        let key = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        guard let revision = try? authority().permissionAuthorizationRevision(productId: productId) else { return }
        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }
        oneTimeGrants[key] = revision
    }

    func consumeOneTimeGrant(productId: String, permission: ProductPermission) -> Bool {
        let key = ProductPermissionGrant.makeIdentifier(
            productId: productId,
            permission: permission
        )

        guard let revision = try? authority().permissionAuthorizationRevision(productId: productId) else {
            return false
        }
        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }
        return oneTimeGrants.removeValue(forKey: key) == revision
    }

    func promptPermissions(
        productId: String,
        permissions: [ProductPermission],
        requester: ProductPermissionRequesting
    ) async throws -> Bool {
        let runtime = try authority()
        let revision = try runtime.permissionAuthorizationRevision(productId: productId)
        let decision: Products.PermissionDecision
        if permissions.count == 1, let permission = permissions.first {
            decision = await requester.prompt(productId: productId, permission: permission)
        } else {
            decision = await requester.promptBatched(productId: productId, permissions: permissions)
        }
        for permission in permissions {
            if let request = try permission.authorizationRequest(), decision != .allowOnce {
                guard try await runtime.setPermissionAuthorizationStatusIfCurrent(
                    productId: productId,
                    request: request,
                    status: decision == .allowAlways ? .authorized : .denied,
                    revision: revision
                ) else { return false }
            } else {
                guard try runtime.permissionAuthorizationRevision(productId: productId) == revision else {
                    return false
                }
                switch decision {
                case .allowAlways:
                    try await grant(productId: productId, permission: permission)
                case .deny:
                    try await deny(productId: productId, permission: permission)
                case .allowOnce:
                    storeOneTime(productId: productId, permission: permission, revision: revision)
                }
            }
        }
        return decision != .deny
    }
}

private extension ProductPermissionRepository {
    func hasOneTimeGrant(for key: String, productId: String) -> Bool {
        guard let revision = try? authority().permissionAuthorizationRevision(productId: productId) else {
            return false
        }
        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }

        return oneTimeGrants[key] == revision
    }

    func clearOneTimeGrant(for key: String) {
        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }

        oneTimeGrants.removeValue(forKey: key)
    }

    func storeOneTime(productId: String, permission: ProductPermission, revision: UInt64) {
        let key = ProductPermissionGrant.makeIdentifier(productId: productId, permission: permission)
        oneTimeLock.lock()
        defer { oneTimeLock.unlock() }
        oneTimeGrants[key] = revision
    }

    func legacyGrants(productId: String) async throws -> [ProductPermissionGrant] {
        try await productRepository(for: productId).fetchAllOperation(with: .init()).asyncExecute()
    }

    func canonicalEntries(
        productId: String,
        legacy: [ProductPermissionGrant]
    ) async throws -> [PermissionAuthorizationEntry] {
        let runtime = try authority()
        let entries = try legacy.compactMap { grant -> PermissionAuthorizationEntry? in
            guard let request = try grant.permission.authorizationRequest() else { return nil }
            return PermissionAuthorizationEntry(request: request, status: grant.granted ? .authorized : .denied)
        }
        if !entries.isEmpty {
            _ = try await runtime.importPermissionAuthorizations(productId: productId, entries: entries)
        }
        return try await runtime.permissionAuthorizations(productId: productId)
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

enum ProductPermissionMappingError: LocalizedError {
    case runtimeUnavailable
    case unsupported(String, String)
    case invalidDomain(String)

    var errorDescription: String? {
        switch self {
        case .runtimeUnavailable:
            "The native permission runtime is unavailable. Permissions have not been changed."
        case let .unsupported(type, key):
            "Cannot map stored permission \(type): \(key)."
        case let .invalidDomain(domain):
            "Cannot map stored network permission: \(domain)."
        }
    }
}

extension ProductPermission {
    func canonicalPermission() throws -> ProductPermission {
        switch self {
        case let .networkAccess(domain):
            return .networkAccess(domain: try Self.canonicalDomain(domain))
        case let .networkAccessBundle(domains):
            return .networkAccessBundle(domains: try Array(Set(domains.map { try Self.canonicalDomain($0) })).sorted())
        case let .accountAccess(target):
            return .accountAccess(targetProductId: Self.bareProductLabel(target))
        default:
            return self
        }
    }

    func authorizationRequest() throws -> PermissionAuthorizationRequest? {
        let permission = try canonicalPermission()
        switch permission {
        case let .deviceCapability(capability):
            return .device(capability.authorizationRequest)
        case let .accountAccess(target):
            return .accountAccess(targetProductId: target)
        case .userIdentityAccess:
            return .identityDisclosure
        case .chatAuthority:
            return .chatAuthority
        case .profileDisclosure:
            return .profileDisclosure
        case let .statementStoreAllowance(derivationIndex):
            let selector: TrUAPIHostDerivationIndex? = try derivationIndex.map { selector in
                switch selector {
                case let .index(index):
                    return .index(index)
                case let .raw(bytes):
                    guard bytes.count == 32 else {
                        throw TrUAPIReviewMappingError.invalidDerivationIndexLength(bytes.count)
                    }
                    return .raw(bytes)
                }
            }
            return .statementStoreAllowance(derivationIndex: selector)
        case .balanceAccess:
            return nil
        case .networkAccess, .networkAccessBundle, .webRtcAccess,
             .chainSubmitAccess, .preimageSubmitAccess, .statementSubmitAccess, .jamPeersAccess:
            return try permission.remoteAuthorizationRequest()
        }
    }

    private func remoteAuthorizationRequest() throws -> PermissionAuthorizationRequest {
        switch self {
        case let .networkAccess(domain):
            return .remote(.init(permission: .remote(domains: [domain])))
        case let .networkAccessBundle(domains):
            return .remote(.init(permission: .remote(domains: domains)))
        case .webRtcAccess:
            return .remote(.init(permission: .webRtc))
        case .chainSubmitAccess:
            return .remote(.init(permission: .chainSubmit))
        case .preimageSubmitAccess:
            return .remote(.init(permission: .preimageSubmit))
        case .statementSubmitAccess:
            return .remote(.init(permission: .statementSubmit))
        case let .jamPeersAccess(genesis):
            let bytes = try Data(hexString: genesis)
            guard bytes.count == 32 else {
                throw ProductPermissionMappingError.unsupported(typeName, genesis)
            }
            return .remote(.init(permission: .jamPeers(genesis: bytes)))
        default:
            preconditionFailure("Expected a remote permission")
        }
    }

    static func fromAuthorization(_ request: PermissionAuthorizationRequest) throws -> [ProductPermission] {
        switch request {
        case let .device(device):
            return [.deviceCapability(device.deviceCapabilityType)]
        case let .remote(remote):
            if case let .remote(domains) = remote.permission, domains.count != 1 {
                return [try ProductPermission.networkAccessBundle(domains: domains).canonicalPermission()]
            }
            return try remote.permission.toDomainRequest().toDomainPermissions().map { try $0.canonicalPermission() }
        case .identityDisclosure:
            return [.userIdentityAccess]
        case .chatAuthority:
            return [.chatAuthority]
        case .profileDisclosure:
            return [.profileDisclosure]
        case let .statementStoreAllowance(derivationIndex):
            return [try .statementStoreAllowance(derivationIndex: derivationIndex?.toSelector())]
        case let .accountAccess(targetProductId):
            return [.accountAccess(targetProductId: bareProductLabel(targetProductId))]
        }
    }

    static func bareProductLabel(_ value: String) -> String {
        let parts = ProductStorageKey.normalize(value).split(separator: ".", omittingEmptySubsequences: false)
        return String(parts.count > 1 ? parts[parts.count - 2] : parts[0])
    }

    private static func canonicalDomain(_ value: String) throws -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed == "*" { return trimmed }
        let wildcard = trimmed.hasPrefix("*.")
        var host = wildcard ? String(trimmed.dropFirst(2)) : trimmed
        if host.hasSuffix(".") { host.removeLast() }
        guard !host.isEmpty, !host.contains("/"), !host.contains("@"),
              let url = URL(string: "https://\(host)"),
              let normalized = url.host, url.port == nil, url.query == nil, url.fragment == nil else {
            throw ProductPermissionMappingError.invalidDomain(value)
        }
        return (wildcard ? "*." : "") + normalized.lowercased()
    }
}

private extension DeviceCapabilityType {
    var authorizationRequest: HostDevicePermissionRequest {
        switch self {
        case .camera: .camera
        case .microphone: .microphone
        case .notifications: .notifications
        case .bluetooth: .bluetooth
        case .nfc: .nfc
        case .location: .location
        case .clipboard: .clipboard
        case .openUrl: .openUrl
        case .biometrics: .biometrics
        }
    }
}
