import struct TrUAPIHost.ProductId
import Foundation
import os
import IssueMonitoring
import Testing
@testable import Products

struct ProductResolverTests {
    @Test func fallsBackToALegacyAppWhenNoRootManifestIsPublished() async throws {
        let resolved = try await makeResolver(StubProductMetadataRepository()).resolve(ProductId("hackm3.dot"))

        #expect(!resolved.hasManifest)
        #expect(resolved.displayName == "hackm3.dot")
        #expect(resolved.icon == nil)
        #expect(resolved.executables.app?.identifier.rawValue == "hackm3.dot")
        #expect(resolved.executables.worker == nil)
        #expect(try resolved.contentId(for: .app).rawValue == "hackm3.dot")
    }

    @Test func propagatesUnavailableNetworkBeforeReadingRecords() async throws {
        let stub = StubProductMetadataRepository()
        let resolver = ProductResolver(
            metadataRepository: stub,
            hostProvider: ProductHostFactory(tldProvider: FailingTldProvider()),
            logger: SilentLogger(),
            screening: .passingThrough
        )

        await #expect(throws: FailingTldProvider.Unreachable.self) {
            try await resolver.resolve(ProductId("worker.hackm3.dot"))
        }
        #expect(stub.readCount == 0)
    }

    @Test(arguments: ["hackm3.paseo", "worker.hackm3.paseo", "localhost:3000"])
    func rejectsProductsOutsideTheNetworkBeforeReadingRecords(_ input: String) async throws {
        let stub = StubProductMetadataRepository()
        let resolver = makeResolver(stub)

        await #expect(throws: ProductResolutionError.self) {
            try await resolver.resolve(ProductId(input))
        }
        #expect(stub.readCount == 0)
    }

    @Test func readsMetadataAndEveryPublishedExecutable() async throws {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set(Fixtures.worker, name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.hasManifest)
        #expect(resolved.displayName == "HackM3")
        #expect(resolved.description == "A hackathon product")
        #expect(resolved.icon?.cid == "bafyicon")
        #expect(try resolved.contentId(for: .app).rawValue == "app.hackm3.dot")
        #expect(resolved.executables.worker?.entrypoint == "src/worker.js")
    }

    /// An expanded Pocket card runs the widget, so the subname is read: a
    /// product whose widget went unread opens its app instead, which is a
    /// different archive and a different screen.
    @Test func readsTheWidgetAnExpandedCardRuns() async throws {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set(Fixtures.widget, name: "widget.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.executables.widget?.identifier.rawValue == "widget.hackm3.dot")
        #expect(try resolved.contentId(for: .widget).rawValue == "widget.hackm3.dot")
    }

    @Test func legacyAppDoesNotInventWidgetOrWorkerExecutables() async throws {
        let resolved = try await makeResolver(StubProductMetadataRepository()).resolve(ProductId("hackm3.dot"))

        #expect(try resolved.contentId(for: .app).rawValue == "hackm3.dot")
        #expect(throws: ProductResolutionError.self) { try resolved.contentId(for: .widget) }
        #expect(throws: ProductResolutionError.self) { try resolved.contentId(for: .worker) }
    }

    @Test func servesEachPublishedKindFromItsOwnSubname() async throws {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set(Fixtures.widget, name: "widget.hackm3.dot", key: "executable")
        stub.set(Fixtures.worker, name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(try resolved.contentId(for: .app).rawValue == "app.hackm3.dot")
        #expect(try resolved.contentId(for: .widget).rawValue == "widget.hackm3.dot")
        #expect(try resolved.contentId(for: .worker).rawValue == "worker.hackm3.dot")
    }

    @Test func skipsAMalformedExecutableWithoutLosingItsSiblings() async throws {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.app, name: "app.hackm3.dot", key: "executable")
        stub.set("{ not json", name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.executables.app != nil)
        #expect(resolved.executables.worker == nil)
        #expect(throws: ProductResolutionError.self) { try resolved.contentId(for: .worker) }
    }

    /// Chat and Pocket ride on one worker record, so a pocket section the
    /// publisher typed wrong must not take the product's chat off the air.
    @Test func keepsAChatWorkerWhoseProductPublishedAMalformedPocketSection() async throws {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.set(Fixtures.workerWithMalformedPocket, name: "worker.hackm3.dot", key: "executable")

        let resolved = try await makeResolver(stub).resolve(ProductId("hackm3.dot"))

        #expect(resolved.executables.worker?.serves(.chat) == true)
        #expect(resolved.executables.worker?.entrypoint == "src/worker.js")
        #expect(resolved.executables.worker?.pocketCards.isEmpty == true)
    }

    @Test func failsWhenTheRootManifestItselfIsMalformed() async {
        let stub = StubProductMetadataRepository()
        stub.set(#"{"$v":9,"displayName":"x"}"#, name: "hackm3.dot", key: "manifest")

        await #expect(throws: ProductResolutionError.self) {
            try await makeResolver(stub).resolve(ProductId("hackm3.dot"))
        }
    }

    @Test func acceptsASubnameAndResolvesItsBase() async throws {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")

        let resolved = try await makeResolver(stub).resolve(ProductId("worker.hackm3.dot"))

        #expect(resolved.id.rawValue == "hackm3.dot")
        #expect(resolved.displayName == "HackM3")
    }

    @Test func servesRepeatResolutionsFromCache() async throws {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")

        let resolver = makeResolver(stub)
        _ = try await resolver.resolve(ProductId("hackm3.dot"))
        let readsAfterFirst = stub.readCount

        _ = try await resolver.resolve(ProductId("hackm3.dot"))

        #expect(stub.readCount == readsAfterFirst)
    }

    /// Caching a partial read would serve the base name's archive for the rest of the session.
    @Test func propagatesAFailedExecutableReadInsteadOfReportingNoExecutable() async {
        let stub = StubProductMetadataRepository()
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")
        stub.failingNames = ["app.hackm3.dot"]

        await #expect(throws: (any Error).self) {
            try await makeResolver(stub).resolve(ProductId("hackm3.dot"))
        }
    }

    /// A read failure must stay retryable, so it is never cached.
    @Test func doesNotCacheAFailedResolution() async throws {
        let stub = StubProductMetadataRepository()
        stub.readError = URLError(.notConnectedToInternet)

        let resolver = makeResolver(stub)
        _ = try? await resolver.resolve(ProductId("hackm3.dot"))

        stub.readError = nil
        stub.set(Fixtures.root, name: "hackm3.dot", key: "manifest")

        let resolved = try await resolver.resolve(ProductId("hackm3.dot"))

        #expect(resolved.displayName == "HackM3")
    }

    /// One `product.resolved` per product and session: the chain read, and nothing for the answers the
    /// resolver serves from memory after it.
    @Test func aSessionReportsTheChainReadOnly() async throws {
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry.recording(into: recorder)
        let session = UUID()
        let resolver = makeResolver(StubProductMetadataRepository(), telemetry: telemetry)

        try await telemetry.withContext(session: session, trigger: .open) {
            _ = try await resolver.resolve(ProductId("hackm3.dot"))
            _ = try await resolver.resolve(ProductId("hackm3.dot"))
            _ = try await resolver.resolve(ProductId("hackm3.dot"))
        }

        #expect(recorder.events.map { $0.tags["outcome"] } == ["ok"])
        let event = try #require(recorder.events.first)
        #expect(event.tags["product.id"] == "hackm3.dot")
        #expect(event.tags["load.trigger"] == "open")
        #expect(event.durationMilliseconds == 10)
        #expect(event.extras["event.monotonic_ms"] == .integer(20))
        #expect(event.tags["host.session_id"] == session.uuidString.lowercased())
    }

    /// Outside a session the memo is a cache hit, reported once per trigger however often it is asked.
    @Test func aPrewarmReportsTheChainReadAndTheFirstCacheHit() async throws {
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry.recording(into: recorder)
        let resolver = makeResolver(StubProductMetadataRepository(), telemetry: telemetry)

        try await telemetry.withContext(session: nil, trigger: .prewarm) {
            _ = try await resolver.resolve(ProductId("hackm3.dot"))
            _ = try await resolver.resolve(ProductId("hackm3.dot"))
            _ = try await resolver.resolve(ProductId("hackm3.dot"))
        }

        #expect(recorder.events.map { $0.tags["outcome"] } == ["ok", "cache_hit"])
    }

    /// A refresh or prewarm that lands while a page is open is not that page's load.
    @Test func aRefreshInsideAnOpenCarriesNoSession() async throws {
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry.recording(into: recorder)
        let resolver = makeResolver(StubProductMetadataRepository(), telemetry: telemetry)

        try await telemetry.withContext(session: UUID(), trigger: .open) {
            try await telemetry.withContext(session: nil, trigger: .refresh) {
                _ = try await resolver.resolve(ProductId("hackm3.dot"))
            }
        }

        #expect(recorder.events.map { $0.tags["load.trigger"] } == ["refresh"])
        #expect(recorder.events.first?.tags["host.session_id"] == nil)
    }

    @Test func aLookupOutsideAnyLoadReportsNothing() async throws {
        let recorder = RecordingHostEventRecorder()
        let resolver = makeResolver(StubProductMetadataRepository(), telemetry: .recording(into: recorder))

        _ = try await resolver.resolve(ProductId("hackm3.dot"))
        _ = try await resolver.resolve(ProductId("hackm3.dot"))

        #expect(recorder.events.isEmpty)
    }

    @Test func reportsAFailedChainReadAsAnError() async throws {
        let stub = StubProductMetadataRepository()
        stub.readError = URLError(.notConnectedToInternet)
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry.recording(into: recorder)
        let resolver = makeResolver(stub, telemetry: telemetry)

        await telemetry.withContext(session: nil, trigger: .open) {
            _ = try? await resolver.resolve(ProductId("hackm3.dot"))
        }

        #expect(recorder.events.map(\.name) == ["host.product.resolved"])
        #expect(recorder.events.first?.tags["outcome"] == "error")
        #expect(recorder.events.first?.tags["host.session_id"] == nil)
    }

    @Test func reportsANetworkThatCannotBeReadAsAnError() async throws {
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry.recording(into: recorder)
        let resolver = makeResolver(StubProductMetadataRepository(), tld: FailingTldProvider(), telemetry: telemetry)

        await telemetry.withContext(session: nil, trigger: .open) {
            _ = try? await resolver.resolve(ProductId("hackm3.dot"))
        }

        #expect(recorder.events.map { $0.tags["outcome"] } == ["error"])
        #expect(recorder.events.first?.tags["product.id"] == "hackm3.dot")
    }

    /// A tap that joins a prewarm's read waited on it too, so its session gets its own event.
    @Test func aCallerJoiningAnotherLoadsReadReportsItsOwnWait() async throws {
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry.recording(into: recorder)
        let repository = GatedMetadataRepository()
        let resolver = ProductResolver(
            metadataRepository: repository,
            hostProvider: ProductHostFactory(tldProvider: StubTldProvider()),
            logger: SilentLogger(),
            screening: .passingThrough,
            telemetry: telemetry
        )
        let session = UUID()

        let prewarm = Task {
            try await telemetry.withContext(session: nil, trigger: .prewarm) {
                try await resolver.resolve(ProductId("hackm3.dot"))
            }
        }
        await repository.firstReadStarted()
        let open = Task {
            try await telemetry.withContext(session: session, trigger: .open) {
                try await resolver.resolve(ProductId("hackm3.dot"))
            }
        }
        try await Task.sleep(for: .milliseconds(100))
        repository.release()
        _ = try await (prewarm.value, open.value)

        let prewarmed = recorder.events.first { $0.tags["load.trigger"] == "prewarm" }
        let opened = recorder.events.first { $0.tags["load.trigger"] == "open" }
        #expect(recorder.events.count == 2)
        #expect(prewarmed?.tags["outcome"] == "ok")
        #expect(opened?.tags["outcome"] == "ok")
        #expect(opened?.tags["host.session_id"] == session.uuidString.lowercased())
    }

    @Test func aCancelledReadReportsNothing() async throws {
        let stub = StubProductMetadataRepository()
        stub.readError = CancellationError()
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry.recording(into: recorder)
        let resolver = makeResolver(stub, telemetry: telemetry)

        await telemetry.withContext(session: nil, trigger: .open) {
            _ = try? await resolver.resolve(ProductId("hackm3.dot"))
        }

        #expect(recorder.events.isEmpty)
    }
}

private func makeResolver(
    _ stub: StubProductMetadataRepository,
    tld: DotNsTldProviding = StubTldProvider(),
    telemetry: HostLoadTelemetry = .disabled
) -> ProductResolver {
    ProductResolver(
        metadataRepository: stub,
        hostProvider: ProductHostFactory(tldProvider: tld),
        logger: SilentLogger(),
        screening: .passingThrough,
        telemetry: telemetry
    )
}

/// Holds every read until released, so a second caller can join the first one's read.
private final class GatedMetadataRepository: ProductMetadataRepositoryProtocol, @unchecked Sendable {
    private struct Gate {
        var released = false
        var waiting: [CheckedContinuation<Void, Never>] = []
    }

    private let started = AsyncStream<Void>.makeStream()
    private let gate = OSAllocatedUnfairLock(initialState: Gate())

    func firstReadStarted() async {
        for await _ in started.stream {
            return
        }
    }

    func release() {
        let waiting = gate.withLock { gate in
            gate.released = true
            defer { gate.waiting = [] }
            return gate.waiting
        }
        waiting.forEach { $0.resume() }
    }

    func fetchMetadata(name _: String, key _: String) async throws -> String? {
        started.continuation.yield(())
        await withCheckedContinuation { continuation in
            let released = gate.withLock { gate in
                if !gate.released { gate.waiting.append(continuation) }
                return gate.released
            }
            if released { continuation.resume() }
        }
        return nil
    }
}

private enum Fixtures {
    static let root = """
    {"$v":1,"displayName":"HackM3","description":"A hackathon product",
     "icon":{"cid":"bafyicon","format":"png"}}
    """

    static let app = #"{"$v":1,"kind":"app","appVersion":[1,0,0]}"#

    static let widget = """
    {"$v":1,"kind":"widget","appVersion":[1,0,0],"dimensions":{"height":[2],"width":4}}
    """

    static let worker = """
    {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"src/worker.js",
     "includes":{"chat":true,"pocket":false}}
    """

    static let workerWithMalformedPocket = """
    {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"src/worker.js",
     "includes":{"chat":true,"pocket":true},"pocket":{"cards":{"loyalty":"Loyalty"}}}
    """
}
