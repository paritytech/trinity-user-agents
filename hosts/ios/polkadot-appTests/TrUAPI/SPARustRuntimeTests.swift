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
    openTap: HostOpenTap? = nil
) -> SPARustRuntime {
    SPARustRuntime(
        executionModel: makeExecutionModel(execution: execution, chainConnections: chainConnections),
        configuration: configuration,
        contentLoader: contentLoader,
        schemeHandlerProxy: SchemeHandlerProxy(),
        logger: Logger.shared,
        hostEventRecorder: hostEventRecorder,
        openTap: openTap
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
        #expect(webViewEvent.tags["host.session_id"] != nil)
        #expect(webViewEvent.extras["event.monotonic_ms"] != nil)

        await runtime.dispose()
    }

    /// The tap minted the id, and the session it opens reports under it, so the tap and the page share one.
    @Test func aSessionTakingATapAdoptsTheIdMintedAtTheTap() async throws {
        let configuration = try makeConfiguration(contentSource: .directURL(#require(URL(string: "http://localhost:3000"))))
        let tap = HostOpenTap(source: .deeplink)
        let hostEventRecorder = RecordingHostEventRecorder()
        let runtime = makeRuntime(configuration: configuration, hostEventRecorder: hostEventRecorder, openTap: tap)

        _ = try await runtime.start(with: MockJSEngine())

        let webViewEvent = try #require(hostEventRecorder.events.first)
        #expect(webViewEvent.tags["host.session_id"] == tap.sessionID.uuidString.lowercased())

        await runtime.dispose()
    }

    @Test func aSessionWithoutATapMintsItsOwnId() async throws {
        let configuration = try makeConfiguration(contentSource: .directURL(#require(URL(string: "http://localhost:3000"))))
        let hostEventRecorder = RecordingHostEventRecorder()
        let runtime = makeRuntime(configuration: configuration, hostEventRecorder: hostEventRecorder)

        _ = try await runtime.start(with: MockJSEngine())

        let sessionID = try #require(hostEventRecorder.events.first?.tags["host.session_id"])
        #expect(UUID(uuidString: sessionID) != nil)
        #expect(sessionID == sessionID.lowercased())

        await runtime.dispose()
    }

    /// The first visible commit sends `host.page.visible` once under the tap's id; a reload or an in-page
    /// navigation commits again and sends nothing.
    @Test func theFirstMainFrameCommitSendsPageVisibleOnce() async throws {
        let configuration = try makeConfiguration(contentSource: .directURL(#require(URL(string: "http://localhost:3000"))))
        let tap = HostOpenTap(source: .pocketCard)
        let hostEventRecorder = RecordingHostEventRecorder()
        let engine = MockJSEngine()
        let runtime = makeRuntime(configuration: configuration, hostEventRecorder: hostEventRecorder, openTap: tap)

        _ = try await runtime.start(with: engine)
        engine.commitMainFrame()
        try await hostEventRecorder.waitForEvents(count: 2)
        await runtime.pageDidCommit(monotonicMilliseconds: HostEventClock.monotonicMilliseconds())
        await runtime.pageDidCommit(monotonicMilliseconds: HostEventClock.monotonicMilliseconds())

        let events = hostEventRecorder.events
        #expect(events.map(\.name) == ["host.webview.created", "host.page.visible"])
        let visible = try #require(events.last)
        #expect(visible.tags["host.session_id"] == tap.sessionID.uuidString.lowercased())
        #expect(visible.tags["open.source"] == "pocket_card")
        #expect(visible.extras["webview_ms"] != nil)
        #expect(visible.extras["tap_ms"] != nil)

        await runtime.dispose()
    }

    /// Each span is the later reading minus the earlier one on the clock `event.monotonic_ms` is read from, so
    /// a test with the subtraction reversed reads -1000 and -700 here and fails.
    @Test func pageVisibleMeasuresFromTheTapAndFromTheWebView() async throws {
        let configuration = try makeConfiguration(contentSource: .directURL(#require(URL(string: "http://localhost:3000"))))
        let tap = HostOpenTap(
            source: .pocketCard,
            openedAtMonotonicMilliseconds: HostEventClock.monotonicMilliseconds() - 1_000
        )
        let hostEventRecorder = RecordingHostEventRecorder()
        let runtime = makeRuntime(configuration: configuration, hostEventRecorder: hostEventRecorder, openTap: tap)

        _ = try await runtime.start(with: MockJSEngine())
        let created = try #require(hostEventRecorder.events.first)
        guard case let .integer(createdAt) = try #require(created.extras["event.monotonic_ms"]) else {
            Issue.record("event.monotonic_ms is not an integer")
            return
        }
        await runtime.pageDidCommit(monotonicMilliseconds: createdAt + 700)

        let visible = try #require(hostEventRecorder.events.last)
        #expect(visible.name == "host.page.visible")
        #expect(created.extras["tap_ms"] == .integer(createdAt - tap.openedAtMonotonicMilliseconds))
        #expect(visible.extras["webview_ms"] == .integer(700))
        #expect(visible.extras["tap_ms"] == .integer(createdAt + 700 - tap.openedAtMonotonicMilliseconds))
        #expect(createdAt - tap.openedAtMonotonicMilliseconds >= 1_000)

        await runtime.dispose()
    }

    @Test func aPageNoTapOpenedBecomesVisibleWithoutATapMeasurement() async throws {
        let configuration = try makeConfiguration(contentSource: .directURL(#require(URL(string: "http://localhost:3000"))))
        let hostEventRecorder = RecordingHostEventRecorder()
        let engine = MockJSEngine()
        let runtime = makeRuntime(configuration: configuration, hostEventRecorder: hostEventRecorder)

        _ = try await runtime.start(with: engine)
        engine.commitMainFrame()
        try await hostEventRecorder.waitForEvents(count: 2)

        let visible = try #require(hostEventRecorder.events.last)
        #expect(visible.name == "host.page.visible")
        #expect(visible.extras["webview_ms"] != nil)
        #expect(visible.extras["tap_ms"] == nil)
        #expect(visible.tags["open.source"] == nil)

        await runtime.dispose()
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

    /// A page a tap opened reads when and how from `__trinityHostTelemetry`, with the schema version unchanged.
    /// A page nothing tapped has neither field, as the bootstrap test above shows.
    @Test func bootstrapCarriesTheTapThatOpenedThePage() throws {
        let origin = try HostEventOrigin(
            launchID: #require(UUID(uuidString: "18a5320e-3a4d-46df-ad7a-ebba4ef3ee3f")),
            osVersion: "18.1",
            appVersion: "1.2.3",
            appBuild: 456,
            appVariant: "nightly"
        )
        let hostSessionID = try #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))
        let openedAt = Date(timeIntervalSince1970: 1_793_900_000.123)
        let sources: [(HostOpenTap.Source, String)] = [(.pocketCard, "pocket_card"), (.deeplink, "deeplink")]

        for (source, name) in sources {
            let telemetry = NativeHostTelemetry(
                hostSessionID: hostSessionID,
                origin: origin,
                truapiHostVersion: "0.9.0",
                openTap: HostOpenTap(source: source, openedAt: openedAt)
            )

            let scripts = try RustRuntimeScriptsFactory(bootstrapScript: "/*bootstrap*/", hostTelemetry: telemetry)
                .makeScripts()

            let identity = #"{"appBuild":"456","appVariant":"nightly","appVersion":"1.2.3","#
                + #""hostSessionId":"d93fe846-8f3c-4cfa-a440-402b6f7b43d3","#
            let tap = #""openSource":"\#(name)","openedAtEpochMs":1793900000123,"#
            let rest = #""osVersion":"18.1","platform":"ios","schemaVersion":"1","truapiHostVersion":"0.9.0"}"#
            #expect(scripts[0].content.contains("Object.freeze(" + identity + tap + rest + ")"))
        }
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
