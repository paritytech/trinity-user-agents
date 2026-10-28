import Products
import TrUAPIHost
import UIKit
import UIKitExt

/// Serves `scanner.scan`: opens the app's scanner over whatever is on screen
/// for a product the user can see, and waits for the code they scanned. The
/// core admits one scan at a time and has already checked that a Worker's
/// card was tapped.
final class AppScannerHostBridge: ScannerHostBridge, @unchecked Sendable {
    typealias ScannerMaking = @MainActor (
        ProductId,
        HostScannerScanRequest,
        @escaping (HostScan) -> Void
    ) -> ControllerBackedProtocol

    private let routerFacade: ProductRoutersFacadeProtocol
    private let visibility: ProductVisibility
    private let makeScanner: ScannerMaking

    init(
        routerFacade: ProductRoutersFacadeProtocol,
        visibility: ProductVisibility,
        makeScanner: @escaping ScannerMaking = ProductScanViewFactory.createView
    ) {
        self.routerFacade = routerFacade
        self.visibility = visibility
        self.makeScanner = makeScanner
    }

    func scanCode(
        productId: String,
        executionKind: ProductExecutionKind,
        request: HostScannerScanRequest
    ) async throws -> HostScan {
        await awaitDecision(cancelled: .dismissed) { [routerFacade, visibility, makeScanner] pending in
            guard await visibility.waitUntilVisible(productId: productId, executionKind: executionKind),
                  !pending.isFinished else {
                return .notVisible
            }
            return await withCheckedContinuation { continuation in
                var presented = true
                let scanner = makeScanner(productId, request) { scan in
                    continuation.resume(returning: presented ? scan : .notVisible)
                }
                pending.screen = scanner.controller
                presented = routerFacade.productsRouter.present(view: scanner)
            }
        }
    }
}
