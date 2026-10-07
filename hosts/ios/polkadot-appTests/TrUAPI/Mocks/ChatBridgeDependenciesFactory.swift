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
    let defaults = UserDefaults(suiteName: "io.parity.tests.chat-bridge") ?? .standard
    return .init(
        productId: productId,
        permissionGuard: MockPermissionGuard(),
        osPermissionAsker: MockOSPermissionAsker(),
        notificationScheduler: MockNotificationScheduler(),
        navigationRouter: MockNavigationRouter(),
        chainRegistry: MockChainRegistry(),
        chainConnections: TrUAPIChainConnectionPool(
            engineResolver: { _ in nil },
            logger: Logger.shared
        ),
        productStorage: TrUAPILocalStorage.createProductLocalStorage(
            productId: productId,
            defaults: defaults
        ),
        coreStorage: TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults),
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
    func resolvePage(destination: String) async throws -> ProductPage {
        throw ProductPageResolutionError.destinationNotOnNetwork(destination: destination, tld: "")
    }
}
