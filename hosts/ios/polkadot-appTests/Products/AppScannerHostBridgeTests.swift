import Products
import Testing
import TrUAPIHost
import UIKit
import UIKitExt
@testable import polkadot_app

/// The core admits one scan at a time and frees the slot when it stops
/// waiting, so a scanner left open after a cancel would sit over the next one.
/// A scanner that opens by mistake waits for an answer forever.
@Suite(.timeLimit(.minutes(1)))
@MainActor
struct AppScannerHostBridgeTests {
    private let request = HostScannerScanRequest(formats: [.qr], prefix: nil, hint: nil)

    @Test("A product the user cannot see gets NotVisible and no scanner opens")
    func hiddenProductOpensNothing() async throws {
        let page = StubProductPage(productId: "shop.dot")
        let window = try makeTestWindow(root: page)
        let application = StubApplication()
        let bridge = makeBridge(page: page, application: application, window: window)

        application.applicationState = .background
        let backgroundWorker = try await bridge.scanCode(
            productId: "shop.dot",
            executionKind: .worker,
            request: request
        )
        application.applicationState = .active
        let otherApp = try await bridge.scanCode(productId: "other.dot", executionKind: .app, request: request)

        #expect([backgroundWorker, otherApp] == [.notVisible, .notVisible])
        #expect(page.presentedViewController == nil)
    }

    @Test("A cancelled scan closes its scanner, and the next scan opens straight after")
    func cancelledScanClosesAndNextOpens() async throws {
        let page = StubProductPage(productId: "shop.dot")
        let window = try makeTestWindow(root: page)
        let bridge = makeBridge(page: page, application: StubApplication(), window: window)

        let first = Task { try await bridge.scanCode(productId: "shop.dot", executionKind: .worker, request: request) }
        try await waitUntil { page.presentedViewController is StubScanner }
        weak var firstScanner = page.presentedViewController
        first.cancel()
        let firstAnswer = try await first.value

        let second = Task { try await bridge.scanCode(productId: "shop.dot", executionKind: .worker, request: request) }
        try await waitUntil {
            page.presentedViewController is StubScanner && page.presentedViewController !== firstScanner
        }
        second.cancel()
        _ = try await second.value

        #expect(firstAnswer == .dismissed)
    }

    @Test("A cancelled scan also closes what its scanner opened, such as the Settings alert")
    func cancelledScanClosesWhatItOpened() async throws {
        let page = StubProductPage(productId: "shop.dot")
        let window = try makeTestWindow(root: page)
        let bridge = makeBridge(page: page, application: StubApplication(), window: window)

        let scan = Task { try await bridge.scanCode(productId: "shop.dot", executionKind: .worker, request: request) }
        try await waitUntil { page.presentedViewController is StubScanner }
        let scanner = try #require(page.presentedViewController)
        try await waitUntil { !scanner.isBeingPresented }
        await present(UIViewController(), over: scanner)
        scan.cancel()
        _ = try await scan.value

        try await waitUntil { page.presentedViewController == nil }
    }

    @Test("A scan cancelled while waiting for its page opens nothing")
    func scanCancelledWhileWaitingOpensNothing() async throws {
        let page = StubProductPage(productId: "shop.dot")
        let window = try makeTestWindow(root: page)
        let bridge = makeBridge(page: page, application: StubApplication(), window: window)
        await present(UIViewController(), over: page)
        page.dismiss(animated: true)

        let scan = Task { try await bridge.scanCode(productId: "shop.dot", executionKind: .app, request: request) }
        try await Task.sleep(for: .milliseconds(50))
        scan.cancel()
        _ = try await scan.value
        try await waitUntil { page.presentedViewController == nil }
        try await Task.sleep(for: .milliseconds(200))

        #expect(page.presentedViewController == nil)
    }

    private func makeBridge(
        page: StubProductPage,
        application: StubApplication,
        window: UIWindow
    ) -> AppScannerHostBridge {
        let router = ProductsRouter()
        router.setPresentationView(page)
        return AppScannerHostBridge(
            routerFacade: ProductRoutersFacade(productsRouter: router, navigationRouter: MockNavigationRouter()),
            visibility: ProductVisibility(application: application, window: { window }),
            makeScanner: { _, _, answer in StubScanner(answer: answer) }
        )
    }
}

/// Answers like the real scanner when it goes away unanswered.
private final class StubScanner: UIViewController, ControllerBackedProtocol {
    private let answer: (HostScan) -> Void

    init(answer: @escaping (HostScan) -> Void) {
        self.answer = answer
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    deinit {
        answer(.dismissed)
    }
}
