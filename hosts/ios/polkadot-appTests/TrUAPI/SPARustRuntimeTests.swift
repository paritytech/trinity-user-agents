import Foundation
import Testing
import AsyncExtensions
import IssueMonitoring
import Products
import TrUAPIHost
@testable import polkadot_app

// MARK: - Helpers

private func makeConfiguration(contentSource: SPAContentSource) throws -> SPAConfiguration {
    let tldProvider = StubTldProvider()
    let factory = ProductHostFactory(tldProvider: tldProvider)
    let host = try #require(factory.host(rawString: "test.dot"))

    return SPAConfiguration(
        title: nil,
        isRootScreen: false,
        showMoreButton: false,
        page: ProductPage(host: host),
        contentSource: contentSource
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

    func currentTldElseResolve() async throws -> String {
        if let tld = currentTld() {
            return tld
        }

        return try await resolveTld()
    }
}

private func makeRuntime(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections(),
    configuration: SPAConfiguration,
    contentLoader: ProductContentLoading = StubProductContentLoader(),
    hostEventRecorder: HostEventRecording = NoopHostEventRecorder(),
    sessions: HostSessionRegistry = HostSessionRegistry(),
    monotonicMilliseconds: @escaping @Sendable () -> Int = { HostEventClock.monotonicMilliseconds() }
) -> SPARustRuntime {
    SPARustRuntime(
        executionModel: makeExecutionModel(execution: execution, chainConnections: chainConnections),
        configuration: configuration,
        contentLoader: contentLoader,
        schemeHandlerProxy: SchemeHandlerProxy(),
        logger: Logger.shared,
        telemetry: HostLoadTelemetry(
            recorder: hostEventRecorder,
            sessions: sessions,
            monotonicMilliseconds: monotonicMilliseconds
        )
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

    /// The first page the session loads is reported once, under the session id its WebView was created
    /// with, timed from the runtime opening the session.
    @Test func firstFinishedPageIsReportedOnceUnderItsSession() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let configuration = try makeConfiguration(contentSource: .directURL(directURL))
        let recorder = RecordingHostEventRecorder()
        let clock = SteppingClock(start: 1_000, step: 250)
        let runtime = makeRuntime(
            configuration: configuration,
            hostEventRecorder: recorder,
            monotonicMilliseconds: { clock.read() }
        )

        _ = try await runtime.start(with: MockJSEngine())
        await runtime.didFinishPageLoad(loaded: true)
        await runtime.didFinishPageLoad(loaded: false)

        let events = recorder.events
        #expect(events.map(\.name) == ["host.webview.created", "host.webview.page_loaded"])

        let created = try #require(events.first)
        let pageLoaded = try #require(events.last)
        #expect(pageLoaded.extras["host.session_id"] == created.extras["host.session_id"])
        #expect(pageLoaded.tags["outcome"] == "ok")
        #expect(pageLoaded.tags["product.id"] == "test.dot")
        #expect(pageLoaded.extras["event.monotonic_ms"] == .integer(1_500))
        #expect(pageLoaded.extras["duration_ms"] == .integer(500))

        await runtime.dispose()
    }

    @Test func aFailedFirstPageIsReportedAsAnError() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL)),
            hostEventRecorder: recorder
        )

        _ = try await runtime.start(with: MockJSEngine())
        await runtime.didFinishPageLoad(loaded: false)

        #expect(recorder.events.last?.name == "host.webview.page_loaded")
        #expect(recorder.events.last?.tags["outcome"] == "error")

        await runtime.dispose()
    }

    /// A page that finishes before the runtime handed out its URL, or after it was disposed, is
    /// not this session's page.
    @Test func aPageOutsideTheStartedSessionIsNotReported() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL)),
            hostEventRecorder: recorder
        )

        await runtime.didFinishPageLoad(loaded: true)
        _ = try await runtime.start(with: MockJSEngine())
        await runtime.dispose()
        await runtime.didFinishPageLoad(loaded: true)

        #expect(recorder.events.map(\.name) == ["host.webview.created"])
    }

    /// The product's dotNS reads run before its page starts, so the session opens with the runtime
    /// and serves the product's subnames too; it closes when the runtime is disposed.
    @Test func theSessionIsOpenForTheProductAndItsSubnamesUntilDisposed() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let sessions = HostSessionRegistry()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL)),
            hostEventRecorder: recorder,
            sessions: sessions
        )

        let openSession = try #require(sessions.sessionID(forProductOrSubname: "test.dot"))
        #expect(sessions.sessionID(forProductOrSubname: "app.test.dot") == openSession)

        _ = try await runtime.start(with: MockJSEngine())
        #expect(recorder.events.first?.extras["host.session_id"] == .string(openSession.uuidString.lowercased()))

        await runtime.dispose()

        #expect(sessions.sessionID(forProductOrSubname: "test.dot") == nil)
    }

    #if DEBUG
        /// A device reaches the Mac by its LAN address, but the product runs under the identifier
        /// the core gave it, so the page loads from one and the product runs under the other.
        @Test func devServerLoadsItsAddressAndRunsUnderTheCoresIdentifier() async throws {
            let address = try #require(URL(string: "http://192.168.1.59:3000/app?product_id=humanity.dot"))
            let configuration = try makeConfiguration(
                contentSource: .devServer(url: address, productId: try! ProductId("humanity.dot"))
            )
            let loader = StubProductContentLoader()
            let runtime = makeRuntime(configuration: configuration, contentLoader: loader)

            let url = try await runtime.start(with: MockJSEngine())

            #expect(url == address)
            #expect(loader.openedIds.isEmpty)
            #expect(configuration.productId.rawValue == "humanity.dot")

            await runtime.dispose()
        }
    #endif

    /// The playground opens through a direct URL under its own dotNS name, not as a dev server.
    @Test func directURLKeepsTheProductIdentifierOfItsPage() throws {
        let configuration = try makeConfiguration(
            contentSource: .directURL(#require(URL(string: "http://localhost:3000")))
        )

        #expect(configuration.productId.rawValue == "test.dot")
    }

    @Test func startWithDotNsResolvesContentAndReturnsProductURL() async throws {
        let configuration = try makeConfiguration(contentSource: .dotNs)
        let loader = StubProductContentLoader()
        let engine = MockJSEngine()
        let runtime = makeRuntime(configuration: configuration, contentLoader: loader)

        let url = try await runtime.start(with: engine)

        #expect(try loader.openedIds == [ProductId("test.dot")])
        #expect(url.scheme == productContentScheme())
        #expect(url.host == "test.dot")
        #expect(url.path == "/")
        #expect(engine.initializedScripts[0]
            .content == #"window.__truapi_localhost = { url: "ws://127.0.0.1:0/?t=test" };"#)

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
