import Foundation

public protocol ProductPermissionRequesting: Sendable {
    func prompt(productId: String, permission: ProductPermission) async -> PermissionDecision
    func promptBatched(productId: String, permissions: [ProductPermission]) async -> PermissionDecision
}

/// Serializes permission prompts so that only one sheet is presented at a
/// time. Uses an `actor` to queue prompt calls.
public actor ProductPermissionRequester: ProductPermissionRequesting {
    private let router: ProductPermissionRouting

    public init(router: ProductPermissionRouting) {
        self.router = router
    }

    public func prompt(
        productId: String,
        permission: ProductPermission
    ) async -> PermissionDecision {
        await promptBatched(productId: productId, permissions: [permission])
    }

    public func promptBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async -> PermissionDecision {
        await withCheckedContinuation { continuation in
            Task { @MainActor [router] in
                let context = ProductPermissionContext(
                    productId: productId,
                    permissions: permissions
                )
                context.setContinuation(continuation)
                router.showPrompt(context: context)
            }
        }
    }
}
