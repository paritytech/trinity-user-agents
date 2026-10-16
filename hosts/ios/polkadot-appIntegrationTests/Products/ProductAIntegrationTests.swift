@testable import polkadot_app
import Products
import SDKLogger
import SubstrateSdk
import XCTest
import Keystore_iOS

/// Integration tests using the real product_a.js script.
///
/// Downloads the production product script and exercises the full pipeline:
/// WKWebViewJSEngine → ContainerBridge → HostApi → ProductsScriptExecutor
///
/// The product script initializes asynchronously (handshake via MessageChannel,
/// chat subscription, render handler registration). It sends a welcome message
/// via `chatSendTextMessage` during init — we use that as a readiness signal.
final class ProductAIntegrationTests: XCTestCase {
    private static let productA =
        URL(
            string: "https://gist.githubusercontent.com/valentunn/6e060abccc66d18095a4836fe4e2b500/raw/dbb9276a323582603f91c1b935fb4c22909e67a5/product_a.js"
        )!

    private static let coinFlip = URL(string: "https://paritytech.github.io/coin-flip/worker/index.js")!

    func testProductAInitializes() async throws {
        try await performTestProductScriptInitializes(product: "productA", scriptUrl: Self.productA)
    }

    func testProductARendersMessages() async throws {
        try await performTestRenderMessage(product: "productA", scriptUrl: Self.productA)
    }

    /// Tests that coinFlip initializes and responds to a user message.
    /// Unlike productA, coinFlip doesn't send a welcome message on init —
    /// it only replies when it receives a MessagePosted action.
    func testCoinFlipRespondsToMessage() async throws {
        let productScript = try await downloadProductScript(for: Self.coinFlip)
        let executor = ProductsScriptExecutor.makeExecutorWithMockedStorage(
            productId: "coinFlip",
            productScript: productScript
        )
        let nativeApi = MockProductsNativeApi(
            localStorage: ProductsLocalStorage(productId: "coinFlip", settingsManager: InMemorySettingsManager())
        )

        try await executor.initializeBot(nativeApi: nativeApi)
        try await executor.onBotStarted()

        // CoinFlip only sends messages in response to user input
        try await executor.onUserMessage(text: "flip", roomId: nil)
        try await nativeApi.waitForFirstMessage(timeout: 10)

        guard let message = nativeApi.sentMessages.first else {
            XCTFail("CoinFlip should reply after receiving a user message")
            return
        }

        Logger.shared.debug("Message: \(message)")

        await executor.dispose()
    }

    /// Tests that the real product script initializes and sends its welcome message.
    private func performTestProductScriptInitializes(product: String, scriptUrl: URL) async throws {
        let productScript = try await downloadProductScript(for: scriptUrl)
        let executor = ProductsScriptExecutor.makeExecutorWithMockedStorage(
            productId: product,
            productScript: productScript
        )
        let nativeApi = MockProductsNativeApi(
            localStorage: ProductsLocalStorage(productId: product, settingsManager: InMemorySettingsManager())
        )

        try await executor.initializeBot(nativeApi: nativeApi)
        try await executor.onBotStarted()

        try await nativeApi.waitForFirstMessage(timeout: 10)

        XCTAssertFalse(nativeApi.sentMessages.isEmpty, "Product should send a welcome message during init")

        await executor.dispose()
    }

    /// Tests that renderMessage with the real product script produces a widget update.
    private func performTestRenderMessage(product: String, scriptUrl: URL) async throws {
        let logger = Logger.shared
        let productScript = try await downloadProductScript(for: scriptUrl)
        let executor = ProductsScriptExecutor.makeExecutorWithMockedStorage(
            productId: product,
            productScript: productScript
        )
        let nativeApi = MockProductsNativeApi(
            localStorage: ProductsLocalStorage(productId: product, settingsManager: InMemorySettingsManager())
        )

        try await executor.initializeBot(nativeApi: nativeApi)
        try await executor.onBotStarted()
        try await nativeApi.waitForFirstMessage(timeout: 10)

        logger.debug("First message received")

        guard let welcomeMessage = nativeApi.consumeMessage() else {
            XCTFail("No messages")
            return
        }

        logger.debug("Welcome message: \(welcomeMessage)")

        try await executor.onUserMessage(text: "1", roomId: nil)
        try await nativeApi.waitForFirstMessage(timeout: 10)

        guard
            let customMessage = nativeApi.consumeMessage(),
            case let .custom(messageType, data, _) = customMessage else {
            XCTFail("No messages")
            return
        }

        let json = try JSONDecoder().decode(JSON.self, from: data)
        logger.debug("Custom message: \(json)")

        let messageId = UUID().uuidString

        let stream = await executor.renderMessage(
            messageId: messageId,
            messageType: messageType,
            messageData: data
        )

        let renderTask = Task<Void, Error> {
            for try await scaleHex in stream {
                do {
                    let widget = try ScaleWidget.decode(from: scaleHex)
                    logger.debug("Widget to render: \(widget)")
                } catch {
                    logger.error("Decode attempt failed: \(error)")
                }
            }
        }

        let timeoutTask = Task {
            try await Task.sleep(for: .seconds(10))
            await executor.dispose()
        }

        try await renderTask.value
        timeoutTask.cancel()

        await executor.dispose()
    }

    // MARK: - Helpers

    private func downloadProductScript(for url: URL) async throws -> String {
        let (data, _) = try await URLSession.shared.data(from: url)
        return try XCTUnwrap(String(data: data, encoding: .utf8))
    }
}

private extension ProductsScriptExecutor {
    static func makeExecutorWithMockedStorage(
        productId: String = "test-product",
        productScript: String
    ) -> ProductsScriptExecutorProtocol {
        let storage = MockChatScriptStorage(scripts: [productId: productScript])
        let schemeHandler = ProductScriptSchemeHandler(
            productId: productId,
            entryRelativePath: storage.chatEntrypointRelativePath(),
            productFileProvider: storage
        )

        guard let baseURL = schemeHandler.getBaseUrl(),
              let productUrl = schemeHandler.getProductUrl() else {
            fatalError("Failed to construct URLs for productId: \(productId)")
        }

        return ProductsScriptExecutor(
            productUrl: productUrl,
            scriptsFactory: ChatNativeRuntimeScriptsFactory(),
            engineFactory: {
                WKWebViewJSEngine(
                    engineBaseUrl: baseURL,
                    scriptHandlers: [
                        JSEngineLogger(logger: Logger.shared)
                    ],
                    hostWindowProvider: { nil },
                    urlSchemeHandlers: [
                        ProductScriptSchemeHandler.scheme: schemeHandler
                    ],
                    settings: WKWebViewJSEngineSettings(usesPersistentLocalStorage: false),
                    logger: Logger.shared
                )
            },
            logger: Logger.shared
        )
    }
}
