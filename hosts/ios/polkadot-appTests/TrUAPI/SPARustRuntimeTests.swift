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
    openTap: HostOpenTap? = nil
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
        openTap: openTap.map(SPAOpenTap.init)
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
    /// with, timed from the runtime opening the session to the moment WebKit reported the page.
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
        let finishedAt = try #require(runtime.pageLoadTimestamp())
        await runtime.didFinishPageLoad(loaded: true, at: finishedAt)
        await runtime.didFinishPageLoad(loaded: false, at: finishedAt)

        let events = recorder.events
        #expect(events.map(\.name) == ["host.webview.created", "host.webview.page_loaded"])

        let created = try #require(events.first)
        let pageLoaded = try #require(events.last)
        #expect(pageLoaded.tags["host.session_id"] == created.tags["host.session_id"])
        #expect(pageLoaded.tags["host.session_id"] != nil)
        #expect(pageLoaded.tags["outcome"] == "ok")
        #expect(pageLoaded.tags["product.id"] == "test.dot")
        #expect(pageLoaded.extras["duration_ms"] == .integer(500))
        #expect(pageLoaded.extras["event.monotonic_ms"] == .integer(1_750))

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
        await runtime.didFinishPageLoad(loaded: false, at: 0)

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

        await runtime.didFinishPageLoad(loaded: true, at: 0)
        _ = try await runtime.start(with: MockJSEngine())
        await runtime.dispose()
        await runtime.didFinishPageLoad(loaded: true, at: 0)

        #expect(recorder.events.map { $0.tags["outcome"] } == [nil, "abandoned"])
    }

    /// Closed before its first page finished, so the user waited and gave up.
    @Test func aPageDisposedBeforeItFinishedIsAbandoned() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL)),
            hostEventRecorder: recorder
        )

        _ = try await runtime.start(with: MockJSEngine())
        await runtime.dispose()

        #expect(recorder.events.map(\.name) == ["host.webview.created", "host.webview.page_loaded"])
        #expect(recorder.events.last?.tags["outcome"] == "abandoned")
    }

    /// The interactor reads the product before the runtime starts; that first, cold read is the
    /// page's load and must carry its session.
    @Test func whatRunsAsThePageOpenCarriesItsSession() async throws {
        let recorder = RecordingHostEventRecorder()
        let loader = StubProductContentLoader()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .dotNs),
            contentLoader: loader,
            hostEventRecorder: recorder
        )

        _ = try await runtime.asPageOpen {
            try await loader.open(ProductId("test.dot"), kind: .app, policy: .untilCancelled)
        }
        _ = try await runtime.start(with: MockJSEngine())

        let session = try #require(recorder.events.first?.tags["host.session_id"])
        #expect(loader.openContexts.count == 2)
        #expect(loader.openContexts.allSatisfy { $0?.sessionID?.uuidString.lowercased() == session })
        #expect(loader.openContexts.allSatisfy { $0?.trigger == .open })

        await runtime.dispose()
    }

    /// Serving a newer build is the user reloading, not the product opening.
    @Test func servingALaterBuildIsNotThePageOpen() async throws {
        let loader = StubProductContentLoader()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .dotNs),
            contentLoader: loader,
            hostEventRecorder: RecordingHostEventRecorder()
        )
        _ = try await runtime.start(with: MockJSEngine())

        loader.simulate(appliedContentHash: "01")
        try await runtime.useLatestBuild()

        #expect(loader.openContexts.last == .some(nil))

        await runtime.dispose()
    }

    @Test func anOpenThatFailedBeforeItsPageIsAnError() async throws {
        let recorder = RecordingHostEventRecorder()
        let loader = StubProductContentLoader()
        loader.failOpens(with: URLError(.notConnectedToInternet))
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .dotNs),
            contentLoader: loader,
            hostEventRecorder: recorder
        )

        await #expect(throws: URLError.self) { try await runtime.start(with: MockJSEngine()) }
        await runtime.dispose()

        #expect(recorder.events.map(\.name) == ["host.webview.page_loaded"])
        #expect(recorder.events.first?.tags["outcome"] == "error")
    }

    @Test func aCancelledOpenIsAbandonedNotFailed() async throws {
        let recorder = RecordingHostEventRecorder()
        let loader = StubProductContentLoader()
        loader.failOpens(with: CancellationError())
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .dotNs),
            contentLoader: loader,
            hostEventRecorder: recorder
        )

        await #expect(throws: CancellationError.self) { try await runtime.start(with: MockJSEngine()) }
        await runtime.dispose()

        #expect(recorder.events.map { $0.tags["outcome"] } == ["abandoned"])
    }

    /// Counted from the runtime's creation: the user who leaves while the product still resolves gave up.
    @Test func aPageClosedBeforeItStartedIsAbandoned() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL)),
            hostEventRecorder: recorder
        )

        await runtime.dispose()

        #expect(recorder.events.map(\.name) == ["host.webview.page_loaded"])
        #expect(recorder.events.first?.tags["outcome"] == "abandoned")
    }

    @Test func anOpenThatFailedBeforeStartingIsAnError() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL)),
            hostEventRecorder: recorder
        )

        await runtime.failedToOpen()
        await runtime.dispose()

        #expect(recorder.events.map { $0.tags["outcome"] } == ["error"])
    }

    /// The product's dotNS reads run before its page starts, inside the page's session, so they join
    /// its page in Sentry.
    @Test func theContentLoadRunsUnderThePageSession() async throws {
        let recorder = RecordingHostEventRecorder()
        let loader = StubProductContentLoader()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .dotNs),
            contentLoader: loader,
            hostEventRecorder: recorder
        )

        _ = try await runtime.start(with: MockJSEngine())

        let session = try #require(recorder.events.first?.tags["host.session_id"])
        let context = try #require(loader.openContexts.first ?? nil)
        #expect(context.sessionID?.uuidString.lowercased() == session)
        #expect(context.trigger == .open)

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

    /// The page a tap opened carries the tap, so its load can be timed from the user's finger.
    @Test func aPageATapOpenedReportsTheTimeFromTheTap() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let clock = SteppingClock(start: 1_000, step: 250)
        let tap = HostOpenTap(
            source: .pocketCard,
            openedAtEpochMilliseconds: 1_793_900_000_000,
            monotonicMilliseconds: 400
        )
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL), openTap: tap),
            hostEventRecorder: recorder,
            monotonicMilliseconds: { clock.read() }
        )

        _ = try await runtime.start(with: MockJSEngine())
        await runtime.didFinishPageLoad(loaded: true, at: 2_000)

        let pageLoaded = try #require(recorder.events.last)
        #expect(pageLoaded.extras["tap_ms"] == .integer(1_600))
        #expect(pageLoaded.extras["duration_ms"] == .integer(1_000))

        await runtime.dispose()
    }

    /// A tap that opened a page is timed to that page's load starting, under the page's session, once
    /// however many runtimes a retry builds for it; a link counts as a tap.
    @Test func aTapThatOpenedAPageIsReportedOnceWhenItsLoadStarts() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let clock = SteppingClock(start: 1_000, step: 250)
        let tap = HostOpenTap(
            source: .deeplink,
            openedAtEpochMilliseconds: 1_793_900_000_000,
            monotonicMilliseconds: 700
        )
        let configuration = try makeConfiguration(contentSource: .directURL(directURL), openTap: tap)

        let runtime = makeRuntime(
            configuration: configuration,
            hostEventRecorder: recorder,
            monotonicMilliseconds: { clock.read() }
        )
        let retry = makeRuntime(configuration: configuration, hostEventRecorder: recorder)
        _ = try await runtime.start(with: MockJSEngine())

        let tapped = recorder.events.filter { $0.name == "host.pocket.card_tapped" }
        let created = try #require(recorder.events.first { $0.name == "host.webview.created" })
        #expect(tapped.count == 1)
        #expect(tapped.first?.tags["outcome"] == "opened")
        #expect(tapped.first?.tags["open.source"] == "deeplink")
        #expect(tapped.first?.extras["duration_ms"] == .integer(300))
        #expect(tapped.first?.tags["host.session_id"] == created.tags["host.session_id"])

        await runtime.dispose()
        await retry.dispose()

        // The retry has a session of its own and is not timed from the tap.
        let pages = recorder.events.filter { $0.name == "host.webview.page_loaded" }
        #expect(pages.count == 2)
        #expect(pages.first?.extras["tap_ms"] != nil)
        #expect(pages.last?.extras["tap_ms"] == nil)
        #expect(pages.first?.tags["host.session_id"] != pages.last?.tags["host.session_id"])
    }

    /// A tap older than five minutes is not the user waiting for this page.
    @Test func aStaleTapIsNotHandedToThePage() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let tap = HostOpenTap(source: .pocketCard, openedAtEpochMilliseconds: 0, monotonicMilliseconds: 0)
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL), openTap: tap),
            hostEventRecorder: recorder,
            monotonicMilliseconds: { 300_001 }
        )

        await runtime.dispose()

        #expect(recorder.events.map(\.name) == ["host.webview.page_loaded"])
        #expect(recorder.events.first?.extras["tap_ms"] == nil)
    }

    @Test func aPageNoTapOpenedHasNoTapTime() async throws {
        let directURL = try #require(URL(string: "http://localhost:3000"))
        let recorder = RecordingHostEventRecorder()
        let runtime = try makeRuntime(
            configuration: makeConfiguration(contentSource: .directURL(directURL)),
            hostEventRecorder: recorder
        )

        _ = try await runtime.start(with: MockJSEngine())
        await runtime.didFinishPageLoad(loaded: true, at: 0)

        #expect(recorder.events.last?.extras["tap_ms"] == nil)

        await runtime.dispose()
    }

    /// The exact `__trinityHostTelemetry` a page and a worker read, pinned whole so Humanity can use them as
    /// fixtures. The tap fields are optional and additive, so the schema version stays 1.
    @Test(arguments: [
        (
            HostOpenTap(source: .pocketCard, openedAtEpochMilliseconds: 1_793_900_000_123, monotonicMilliseconds: 0)
                as HostOpenTap?,
            #"{"appBuild":"456","appVariant":"nightly","appVersion":"1.2.3","#
                + #""hostSessionId":"d93fe846-8f3c-4cfa-a440-402b6f7b43d3","openSource":"pocket_card","#
                + #""openedAtEpochMs":1793900000123,"osVersion":"18.1","platform":"ios","schemaVersion":"1","#
                + #""truapiHostVersion":"0.9.0"}"#
        ),
        (
            HostOpenTap(source: .deeplink, openedAtEpochMilliseconds: 1_793_900_000_123, monotonicMilliseconds: 0),
            #"{"appBuild":"456","appVariant":"nightly","appVersion":"1.2.3","#
                + #""hostSessionId":"d93fe846-8f3c-4cfa-a440-402b6f7b43d3","openSource":"deeplink","#
                + #""openedAtEpochMs":1793900000123,"osVersion":"18.1","platform":"ios","schemaVersion":"1","#
                + #""truapiHostVersion":"0.9.0"}"#
        ),
        // A page no tap opened, and every worker: a worker is never handed a tap.
        (
            nil,
            #"{"appBuild":"456","appVariant":"nightly","appVersion":"1.2.3","#
                + #""hostSessionId":"d93fe846-8f3c-4cfa-a440-402b6f7b43d3","osVersion":"18.1","platform":"ios","#
                + #""schemaVersion":"1","truapiHostVersion":"0.9.0"}"#
        )
    ])
    func theBootstrapIsExactly(tap: HostOpenTap?, json: String) throws {
        let origin = try HostEventOrigin(
            launchID: #require(UUID(uuidString: "18a5320e-3a4d-46df-ad7a-ebba4ef3ee3f")),
            osVersion: "18.1",
            appVersion: "1.2.3",
            appBuild: 456,
            appVariant: "nightly"
        )
        let session = try #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))

        let script = try RustRuntimeScriptsFactory(
            bootstrapScript: "",
            hostTelemetry: NativeHostTelemetry(
                hostSessionID: session,
                origin: origin,
                truapiHostVersion: "0.9.0",
                openTap: tap
            )
        ).makeScripts()[0].content

        #expect(script.contains("value: Object.freeze(\(json)),"))
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
