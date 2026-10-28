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

struct SPARustRuntimeTests {
    @Test func startWithDirectURLStartsBridgeAndInitializesEngine() async throws {
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
        #expect(execution.startWsBridgeCallCount == 1)
        #expect(execution.permissionRequests.isEmpty)
        #expect(engine.mediaHandlerWasInstalledAtInitialization)

        // Tests build IssueMonitoring without Sentry: bootstrap → container → zoom disable.
        #expect(engine.initializedScripts.count == 3)
        #expect(engine.initializedScripts[0]
            .content == #"window.__truapi_localhost = { url: "ws://127.0.0.1:0/?t=test" };"#)
        #expect(engine.initializedScripts[0].insertionPoint == .atDocStart)
        #expect(!engine.initializedScripts[0].content.contains("__truapi_policy__"))
        #expect(engine.initializedScripts[1].content.contains("freezeAndDelete"))
        #expect(engine.initializedScripts[1].insertionPoint == .atDocStart)
        #expect(engine.initializedScripts[2].content.contains("viewport"))
        #expect(engine.initializedScripts[2].insertionPoint == .atDocEnd)
        #expect(engine.initializedScripts[2].frameScope == .mainFrameOnly)

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
        #expect(engine.initializedScripts[0]
            .content == #"window.__truapi_localhost = { url: "ws://127.0.0.1:0/?t=test" };"#)

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

    @Test func useLatestBuildServesTheAppliedBuildWhenItChanged() async throws {
        let configuration = try makeConfiguration(contentSource: .dotNs)
        let loader = StubProductContentLoader()
        let runtime = makeRuntime(configuration: configuration, contentLoader: loader)
        _ = try await runtime.start(with: MockJSEngine())

        loader.simulate(appliedContentHash: "01")
        try await runtime.useLatestBuild()

        #expect(await runtime.servedBuild()?.contentHash == "01")

        await runtime.dispose()
    }

    @Test func useLatestBuildKeepsTheServedBuildWhenNothingChanged() async throws {
        let configuration = try makeConfiguration(contentSource: .dotNs)
        let loader = StubProductContentLoader()
        let runtime = makeRuntime(configuration: configuration, contentLoader: loader)
        _ = try await runtime.start(with: MockJSEngine())
        let served = await runtime.servedBuild()

        try await runtime.useLatestBuild()

        #expect(await runtime.servedBuild() == served)
        #expect(loader.openedIds.count == 2)

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

    @Test func rustScriptsFactoryOrdersBootstrapBeforeContainer() throws {
        let hostSessionID = try #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))
        let origin = try HostEventOrigin(
            launchID: #require(UUID(uuidString: "18a5320e-3a4d-46df-ad7a-ebba4ef3ee3f")),
            osVersion: "18.1",
            appVersion: "1.2.3",
            appBuild: 456,
            appVariant: "nightly"
        )
        let telemetry = NativeHostTelemetry(
            hostSessionID: hostSessionID,
            origin: origin,
            truapiHostVersion: "0.9.0"
        )
        let factory = RustRuntimeScriptsFactory(
            bootstrapScript: "/*bootstrap*/",
            hostTelemetry: telemetry
        )

        let scripts = try factory.makeScripts()
        let expectedJSON = [
            #"{"appBuild":"456","appVariant":"nightly","appVersion":"1.2.3","#,
            #""hostSessionId":"d93fe846-8f3c-4cfa-a440-402b6f7b43d3","osVersion":"18.1","#,
            #""platform":"ios","schemaVersion":"1","truapiHostVersion":"0.9.0"}"#
        ].joined()

        #expect(scripts.count == 3)
        let expectedTelemetryScript = """
        try {
            Object.defineProperty(globalThis, "__trinityHostTelemetry", {
                value: Object.freeze(\(expectedJSON)),
                writable: false,
                configurable: false
            });
        } catch (error) {
            console.warn("Native host telemetry bootstrap was not installed", error);
        }
        """

        #expect(scripts[0].content == expectedTelemetryScript)
        #expect(scripts[0].insertionPoint == .atDocStart)
        #expect(scripts[0].frameScope == .mainFrameOnly)
        #expect(scripts[1].content == "/*bootstrap*/")
        #expect(scripts[1].insertionPoint == .atDocStart)
        #expect(scripts[1].frameScope == .mainFrameOnly)
        #expect(scripts[2].content.contains("__truapi_localhost"))
        #expect(scripts[2].insertionPoint == .atDocStart)
        #expect(scripts[2].frameScope == .allFrames)
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
