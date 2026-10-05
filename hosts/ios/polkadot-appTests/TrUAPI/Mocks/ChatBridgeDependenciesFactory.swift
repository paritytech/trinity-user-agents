import ChainRegistry
import Foundation
import Products
@testable import polkadot_app

/// Minimal dependencies for a chat bridge under test: only the chat callbacks
/// are exercised, so the rest are inert stand-ins.
@MainActor
func makeChatBridgeDependencies(
    productId: String = "test.dot"
) -> RustProductExecutionBridge.Dependencies {
    return .init(
        productId: productId,
        permissionRequester: MockPermissionGuard(),
        osPermissionAsker: MockOSPermissionAsker(),
        notificationScheduler: MockNotificationScheduler(),
        navigationRouter: MockNavigationRouter(),
        chainRegistry: MockChainRegistry(),
        chainConnections: TrUAPIChainConnectionPool(
            engineResolver: { _ in nil },
            logger: Logger.shared
        ),
        confirmationPresenter: MockConfirmationPresenter(),
        preimageCache: TrUAPIPreimageCache { _ in nil },
        hostProvider: InertHostProvider(),
        logger: Logger.shared
    )
}

/// The repo keeps these per-file: `StubHostProvider` already exists twice, with
/// two different shapes.
private struct InertHostProvider: ProductHostProviding {
    func host(rawString _: String) -> ProductHost? { nil }
    func host(url _: URL) -> ProductHost? { nil }
    func host(navigationDestination _: String) -> ProductHost? { nil }
    func host(label _: String) -> ProductHost? { nil }
    func page(url _: URL) -> ProductPage? { nil }
    func page(navigationDestination _: String) -> ProductPage? { nil }
    func resolveHost(label _: String) async throws -> ProductHost? { nil }
    func resolveHost(rawString _: String) async throws -> ProductHost? { nil }
    func resolvePage(destination _: String) async throws -> ProductPage? { nil }
}
