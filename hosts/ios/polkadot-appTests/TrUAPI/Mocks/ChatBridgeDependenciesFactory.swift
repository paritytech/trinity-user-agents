import ChainRegistry
import Foundation
import Products
import DesignSystem
@testable import polkadot_app

/// Minimal dependencies for a chat bridge under test: only the chat callbacks
/// are exercised, so the rest are inert stand-ins.
@MainActor
func makeChatBridgeDependencies(
    productId: String = "test.dot",
    chainConnections: TrUAPIChainConnecting? = nil,
    osPermissionAsker: OSPermissionAsking = MockOSPermissionAsker()
) -> RustProductExecutionBridge.Dependencies {
    let suiteName = "io.parity.tests.chat-bridge"
    let defaults = UserDefaults(suiteName: suiteName)!
    return .init(
        productId: productId,
        permissionGuard: MockPermissionGuard(),
        osPermissionAsker: osPermissionAsker,
        notificationScheduler: MockNotificationScheduler(),
        gameReminders: MockGameReminderScheduler(),
        reminderPermissionAsker: MockReminderPermissionAsker(),
        navigationRouter: MockNavigationRouter(),
        chainRegistry: MockChainRegistry(),
        chainConnections: chainConnections ?? TrUAPIChainConnectionPool(
            engineResolver: { _ in nil },
            logger: Logger.shared
        ),
        productStorage: TrUAPILocalStorage.createProductLocalStorage(
            productId: productId,
            defaults: defaults, storageDomain: suiteName
        ),
        coreStorage: TrUAPILocalStorage.createCoreLocalStorage(defaults: defaults, storageDomain: suiteName),
        confirmationPresenter: MockConfirmationPresenter(),
        chatFiles: UnavailableNativeChatFiles(),
        preimageCache: TrUAPIPreimageCache { _ in nil },
        hostProvider: InertHostProvider(),
        themeManager: ThemeManager.shared,
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
