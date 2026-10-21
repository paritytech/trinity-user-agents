import Foundation

public protocol ProductPermissionRequesting: Sendable {
    /// Asks the user. Throws ``ProductPermissionPromptError`` when no screen gets an answer: that is not a refusal.
    func prompt(productId: String, permission: ProductPermission) async throws -> PermissionDecision
    /// Asks the user once for all `permissions`. Throws as ``prompt(productId:permission:)`` does.
    func promptBatched(productId: String, permissions: [ProductPermission]) async throws -> PermissionDecision
}

/// Shows the permission prompt and waits for the answer of the user.
///
/// A request can come from a product with no screen, for example a chat worker. When no prompt can get an answer,
/// the request fails with ``ProductPermissionPromptError`` and does not wait. A wait with no prompt on the screen
/// holds the permission lock of the core, so it stops all later requests of the product.
public actor ProductPermissionRequester: ProductPermissionRequesting {
    /// The time that a request waits for the app to come to the foreground.
    public static let foregroundWait: Duration = .seconds(5)

    private let router: ProductPermissionRouting
    private let foreground: ProductPromptForegroundWaiting
    private let foregroundWait: Duration

    public init(
        router: ProductPermissionRouting,
        foreground: ProductPromptForegroundWaiting = AlwaysInForeground(),
        foregroundWait: Duration = ProductPermissionRequester.foregroundWait
    ) {
        self.router = router
        self.foreground = foreground
        self.foregroundWait = foregroundWait
    }

    public func prompt(
        productId: String,
        permission: ProductPermission
    ) async throws -> PermissionDecision {
        try await promptBatched(productId: productId, permissions: [permission])
    }

    public func promptBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> PermissionDecision {
        // The wait lets a product that starts with the app ask when the first screen shows.
        guard await foreground.waitForForeground(timeout: foregroundWait) else {
            throw ProductPermissionPromptError.notInForeground
        }

        return try await withCheckedThrowingContinuation { continuation in
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
