import Foundation
import AsyncExtensions
import Products
import TrUAPIHost

/// Fans the core's change notice out to every screen listing stored decisions.
final class TrUAPIPermissionChangeNotifier: Sendable {
    private let subject = AsyncPassthroughSubject<ProductId>()

    var changes: AnyAsyncSequence<ProductId> {
        subject.eraseToAnyAsyncSequence()
    }

    func notify(productId: ProductId) {
        subject.send(productId)
    }
}

/// The runtime surface stored decisions are administered through.
protocol TrUAPIPermissionAdministering: Sendable {
    func permissionAuthorizations() async throws -> [ProductPermissionAuthorization]

    func setPermissionAuthorizationStatus(
        productId: String,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus
    ) throws
}

extension TrUAPIHostRuntime: TrUAPIPermissionAdministering {}

/// Standing grants the rust core stores, listed and revoked without opening
/// the product. Nothing is mirrored natively: the core is read each time.
protocol TrUAPIPermissionAuthorizing: Sendable {
    /// Every grant the core holds, across products.
    func grants() async throws -> [ProductPermissionGrant]

    /// Clear the core's decisions on `permissions`, so the product prompts again.
    func reset(productId: ProductId, permissions: [ProductPermission]) throws

    /// The products whose stored decisions moved, as the core reports them.
    var changes: AnyAsyncSequence<ProductId> { get }
}

final class TrUAPIPermissionAuthorizations: TrUAPIPermissionAuthorizing, @unchecked Sendable {
    typealias RuntimeAccess = @Sendable () throws -> TrUAPIPermissionAdministering

    let changes: AnyAsyncSequence<ProductId>

    private let runtime: RuntimeAccess

    init(runtime: @escaping RuntimeAccess, changes: AnyAsyncSequence<ProductId>) {
        self.runtime = runtime
        self.changes = changes
    }

    convenience init(
        runtimeProvider: TrUAPIHostRuntimeProviding,
        changeNotifier: TrUAPIPermissionChangeNotifier
    ) {
        self.init(runtime: { try runtimeProvider.sharedRuntime() }, changes: changeNotifier.changes)
    }

    func grants() async throws -> [ProductPermissionGrant] {
        try await runtime().permissionAuthorizations().flatMap { authorization -> [ProductPermissionGrant] in
            guard authorization.status == .authorized else { return [] }

            return authorization.request.productPermissions.map { permission in
                ProductPermissionGrant(
                    productId: authorization.productId,
                    permission: permission,
                    granted: true,
                    grantedAt: nil
                )
            }
        }
    }

    func reset(productId: ProductId, permissions: [ProductPermission]) throws {
        guard !permissions.isEmpty else { return }

        let runtime = try runtime()

        for permission in permissions {
            try runtime.setPermissionAuthorizationStatus(
                productId: productId,
                request: PermissionAuthorizationRequest(permission),
                status: .notDetermined
            )
        }
    }
}
