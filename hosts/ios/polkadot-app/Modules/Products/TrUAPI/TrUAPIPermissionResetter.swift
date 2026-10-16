import Products
import TrUAPIHost

protocol ProductPermissionResetting: Sendable {
    func reset(productId: ProductId, permissions: [ProductPermission]) throws
}

final class TrUAPIPermissionResetter: ProductPermissionResetting, @unchecked Sendable {
    typealias ExecutionOpening = @Sendable (ProductId) throws -> TrUAPIProductExecutionProtocol

    private let openExecution: ExecutionOpening

    init(openExecution: @escaping ExecutionOpening) {
        self.openExecution = openExecution
    }

    convenience init(runtimeProvider: TrUAPIHostRuntimeProviding) {
        self.init { productId in
            try runtimeProvider.sharedRuntime().openProductExecution(
                bridge: PermissionAdminBridge(productId: productId),
                configuration: ProductExecutionConfig(productId: productId, executionKind: .app)
            )
        }
    }

    func reset(productId: ProductId, permissions: [ProductPermission]) throws {
        guard !permissions.isEmpty else { return }

        let execution = try openExecution(productId)
        defer { execution.close() }

        for permission in permissions {
            try execution.setPermissionAuthorizationStatus(
                request: PermissionAuthorizationRequest(permission),
                status: .notDetermined
            )
        }
    }
}

private final class PermissionAdminBridge: HostBridge, @unchecked Sendable {
    let storage: HostStorageBackend
    let coreStorage: HostCoreStorageBackend

    init(productId: ProductId) {
        storage = ProductStorageBackend(storage: TrUAPILocalStorage.createProductLocalStorage(productId: productId))
        coreStorage = CoreStorageBackend(storage: TrUAPILocalStorage.createCoreLocalStorage())
    }

    func navigateTo(url _: String) async throws {
        throw HostNavigateToError.Unknown(reason: "no page is open")
    }

    func devicePermission(
        product _: ProductExecutionConfig,
        request _: HostDevicePermissionRequest
    ) async throws -> TrUAPIPermissionDecision {
        .deny
    }

    func remotePermission(
        product _: ProductExecutionConfig,
        request _: RemotePermission
    ) async throws -> TrUAPIPermissionDecision {
        .deny
    }

    func featureSupported(request _: HostFeatureSupportedRequest) async throws -> Bool {
        false
    }
}
