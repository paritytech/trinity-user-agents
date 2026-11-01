import Foundation
import Testing
import AsyncExtensions
import IssueMonitoring
import Products
import TrUAPIHost
@testable import polkadot_app

// MARK: - Helpers

private func makeConfiguration(
    contentSource: SPAContentSource,
    cardFace: (any ExpandedCardFaceShowing)? = nil
) throws -> SPAConfiguration {
    let tldProvider = StubTldProvider()
    let factory = ProductHostFactory(tldProvider: tldProvider)
    let host = try #require(factory.host(rawString: "test.dot"))

    return SPAConfiguration(
        title: nil,
        isRootScreen: false,
        showMoreButton: false,
        page: ProductPage(host: host),
        contentSource: contentSource,
        cardFace: cardFace
    )
}

private struct StubTldProvider: DotNsTldProviding {
    func currentTld() -> String? {
        "dot"
    }

    func resolveTld() async throws -> String {
        "dot"
    }

    func refresh() {}

    func reset() {}
}

@MainActor
private func makeRuntime(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections(),
    configuration: SPAConfiguration,
    contentLoader: ProductContentLoading = StubProductContentLoader(),
    hostEventRecorder: HostEventRecording = NoopHostEventRecorder()
) -> SPARustRuntime {
    SPARustRuntime(
        executionModel: makeExecutionModel(execution: execution, chainConnections: chainConnections),
        configuration: configuration,
        contentLoader: contentLoader,
        schemeHandlerProxy: SchemeHandlerProxy(),
        logger: Logger.shared,
        hostEventRecorder: hostEventRecorder
    )
}

// MARK: - Tests

@MainActor
struct SPARustRuntimeTests {
    @Test func startWithDirectURLReturnsProductURL() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let configuration = try makeConfiguration(contentSource: .directURL(directURL))
        let execution = MockProductExecution()
        let engine = MockJSEngine()
        let hostEventRecorder = RecordingHostEventRecorder()
        let runtime = makeRuntime(
            execution: execution,
            configuration: configuration,
            hostEventRecorder: hostEventRecorder
        )

        let url = try await runtime.start(with: engine)

        #expect(url == directURL)
        #expect(execution.permissionRequests.isEmpty)
        let capture = try #require(engine.deviceCapabilityHandler)
        #expect(try await capture(.camera) == .denied)
        #expect(try await capture(.microphone) == .denied)

        let webViewEvent = try #require(hostEventRecorder.events.first)
        #expect(webViewEvent.name == "host.webview.created")
        #expect(webViewEvent.tags["host.platform"] == "ios")
        #expect(webViewEvent.extras["host.session_id"] != nil)
        #expect(webViewEvent.extras["event.monotonic_ms"] != nil)

        await runtime.dispose()
    }

    #if DEBUG
        /// A device reaches the Mac by its LAN address, but the product runs under the identifier
        /// the core gave it, so the page loads from one and the product runs under the other.
        @Test func devServerLoadsItsAddressAndRunsUnderTheCoresIdentifier() async throws {
            let address = try #require(URL(string: "http://192.168.1.59:3000/app?product_id=humanity.dot"))
            let configuration = try makeConfiguration(
                contentSource: .devServer(url: address, productId: "humanity.dot")
            )
            let loader = StubProductContentLoader()
            let runtime = makeRuntime(configuration: configuration, contentLoader: loader)

            let url = try await runtime.start(with: MockJSEngine())

            #expect(url == address)
            #expect(loader.openedIds.isEmpty)
            #expect(configuration.productId == "humanity.dot")

            await runtime.dispose()
        }
    #endif

    /// The playground opens through a direct URL under its own dotNS name, not as a dev server.
    @Test func directURLKeepsTheProductIdentifierOfItsPage() throws {
        let configuration = try makeConfiguration(
            contentSource: .directURL(#require(URL(string: "http://localhost:3000")))
        )

        #expect(configuration.productId == "test.dot")
    }

    @Test func startWithDotNsResolvesContentAndReturnsProductURL() async throws {
        let configuration = try makeConfiguration(contentSource: .dotNs)
        let loader = StubProductContentLoader()
        let engine = MockJSEngine()
        let runtime = makeRuntime(configuration: configuration, contentLoader: loader)

        let url = try await runtime.start(with: engine)

        #expect(loader.openedIds == ["test.dot"])
        #expect(url.scheme == ProductScriptSchemeHandler.scheme)
        #expect(url.host == "test.dot")
        #expect(url.path == "/")

        await runtime.dispose()
    }

    /// A card's page loads the product's widget rather than its app, since the
    /// widget is what an expanded card runs.
    @Test @MainActor func startForACardsPageLoadsTheWidget() async throws {
        let configuration = try makeConfiguration(contentSource: .dotNs, cardFace: StubCardFace())
        let widget = ProductExecutable.Widget(
            identifier: "widget.test.dot",
            appVersion: .zero,
            description: nil,
            heights: [1],
            width: 1
        )
        let product = ResolvedProduct(
            id: "test.dot",
            displayName: "Test",
            description: nil,
            icon: nil,
            executables: ProductExecutables(app: nil, widget: widget, worker: nil),
            hasManifest: true
        )
        let runtime = makeRuntime(
            configuration: configuration,
            contentLoader: StubProductContentLoader(product: product)
        )

        _ = try await runtime.start(with: MockJSEngine())

        #expect(await runtime.servedBuild()?.localURL.lastPathComponent == "widget.test.dot")
        await runtime.dispose()
    }

    @Test func disposeTearsDownExecutionOnceAndDestroysEngine() async throws {
        let execution = MockProductExecution()
        let chainConnections = MockChainConnections()
        let configuration = try makeConfiguration(
            contentSource: .directURL(#require(URL(string: "http://localhost:3000")))
        )
        let engine = MockJSEngine()
        let runtime = makeRuntime(
            execution: execution,
            chainConnections: chainConnections,
            configuration: configuration
        )

        _ = try await runtime.start(with: engine)
        await runtime.dispose()
        await runtime.dispose()

        #expect(execution.stopWsBridgeCallCount == 1)
        #expect(execution.closeCallCount == 1)
        #expect(chainConnections.closeAllCallCount == 1)
        #expect(engine.destroyCallCount == 1)
    }

    @Test(.timeLimit(.minutes(1))) func revokedExecutionDestroysOnlyItsEngine() async throws {
        let configuration = try makeConfiguration(
            contentSource: .directURL(#require(URL(string: "http://localhost:3000")))
        )
        let revoked = MockProductExecution()
        let revokedEngine = MockJSEngine()
        let liveEngine = MockJSEngine()
        let runtime = makeRuntime(execution: revoked, configuration: configuration)
        let other = makeRuntime(configuration: configuration)
        let (destroyed, continuation) = AsyncStream.makeStream(of: Void.self)
        revokedEngine.onDestroy = { _ = continuation.yield(()) }
        _ = try await runtime.start(with: revokedEngine)
        _ = try await other.start(with: liveEngine)

        NotificationCenter.default.post(name: .productPermissionAuthorizationsChanged, object: "test.dot")
        #expect(revokedEngine.destroyCallCount == 0)
        #expect(liveEngine.destroyCallCount == 0)
        revoked.close()
        NotificationCenter.default.post(name: .productPermissionAuthorizationsChanged, object: "test.dot")
        var iterator = destroyed.makeAsyncIterator()
        _ = await iterator.next()
        #expect(revokedEngine.destroyCallCount == 1)
        #expect(liveEngine.destroyCallCount == 0)
        await runtime.dispose()
        await other.dispose()
        continuation.finish()
    }


    @Test func directURLHandlerAllowsSameHostInterceptsOthers() throws {
        let handler = try DirectURLNavigationDecisionHandler(
            baseURL: #require(URL(string: "http://localhost:3000"))
        )

        let sameHostURL = try #require(URL(string: "http://localhost:3000/page"))
        #expect(handler.decide(url: sameHostURL) == .allow)

        let externalURL = try #require(URL(string: "https://example.com"))
        #expect(handler.decide(url: externalURL) == .intercept(externalURL))
    }
}
