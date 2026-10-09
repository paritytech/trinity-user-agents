import Foundation
import Testing
import AsyncExtensions
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
}

@MainActor
private func makeRuntime(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections(),
    configuration: SPAConfiguration,
    dotNsResolver: DotNsResolverProtocol = StubDotNsResolver(),
    productResolver: ProductResolving = StubProductResolver()
) -> SPARustRuntime {
    SPARustRuntime(
        executionModel: makeExecutionModel(execution: execution, chainConnections: chainConnections),
        configuration: configuration,
        dotNsResolver: dotNsResolver,
        productResolver: productResolver,
        schemeHandlerProxy: SchemeHandlerProxy(),
        logger: Logger.shared
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
        let runtime = makeRuntime(execution: execution, configuration: configuration)

        let url = try await runtime.start(with: engine)

        #expect(url == directURL)
        #expect(execution.permissionRequests.isEmpty)
        let capture = try #require(engine.deviceCapabilityHandler)
        #expect(try await capture(.camera) == .denied)
        #expect(try await capture(.microphone) == .denied)

        await runtime.dispose()
    }

    @Test func startWithDotNsResolvesContentAndReturnsProductURL() async throws {
        let configuration = try makeConfiguration(contentSource: .dotNs)
        let resolver = StubDotNsResolver()
        let engine = MockJSEngine()
        let runtime = makeRuntime(configuration: configuration, dotNsResolver: resolver)

        let url = try await runtime.start(with: engine)

        #expect(resolver.resolvedNames == ["test.dot"])
        #expect(url.scheme == ProductScriptSchemeHandler.scheme)
        #expect(url.host == "test.dot")
        #expect(url.path == "/")

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
