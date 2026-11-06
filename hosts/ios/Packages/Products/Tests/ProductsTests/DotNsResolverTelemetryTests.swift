import Foundation
import IssueMonitoring
import Testing
@testable import Products

struct DotNsResolverTelemetryTests {
    private let name = "app.hackm3.dot"
    private let contracts = StubDotNsContractApi()
    private let recorder = RecordingHostEventRecorder()
    private let telemetry: HostLoadTelemetry

    init() {
        telemetry = .recording(into: recorder)
        contracts.set(contentHash: Data([0x01, 0x70] as [UInt8] + [UInt8](repeating: 0xAB, count: 32)), name: name)
    }

    @Test func aDownloadReportsItsSizeAndContentHashReadUnderTheCallersSession() async throws {
        let session = UUID()
        let resolver = makeResolver()

        _ = try await telemetry.withContext(session: session, trigger: .open) {
            try await resolver.stageContent(dotNsName: name)
        }

        let event = try #require(recorder.events.first)
        #expect(recorder.events.count == 1)
        #expect(event.name == "host.bulletin.fetch")
        #expect(event.tags["outcome"] == "ok")
        #expect(event.tags["trigger"] == "open")
        #expect(event.tags["dotns.domain"] == nil)
        #expect(event.extras["dotns.domain"] == .string(name))
        #expect(event.extras["bulletin.bytes"] == .integer(Int(StubCarFetcher.downloadedBytes)))
        #expect(event.extras["dotns.content_hash_ms"] == .integer(10))
        #expect(event.extras["duration_ms"] == .integer(10))
        #expect(event.extras["event.monotonic_ms"] == .integer(30))
        #expect(event.tags["host.session_id"] == session.uuidString.lowercased())
        #expect(event.extras["host.session_id"] == .string(session.uuidString.lowercased()))
    }

    @Test func aLookupOutsideAnyLoadReportsNothing() async throws {
        _ = try await makeResolver().stageContent(dotNsName: name)

        #expect(recorder.events.isEmpty)
    }

    @Test func contentAlreadyOnDiskIsACacheHit() async throws {
        let resolver = makeResolver()

        try await telemetry.withContext(session: nil, trigger: .prewarm) {
            _ = try await resolver.stageContent(dotNsName: name)
            _ = try await resolver.stageContent(dotNsName: name)
        }

        #expect(recorder.events.map { $0.tags["outcome"] } == ["ok", "cache_hit"])
        #expect(recorder.events.map { $0.tags["trigger"] } == ["prewarm", "prewarm"])
        #expect(recorder.events.last?.extras["bulletin.bytes"] == nil)
        #expect(recorder.events.last?.tags["host.session_id"] == nil)
    }

    /// The applied archive is read on every render, so it is reported once per session.
    @Test func anAppliedArchiveIsACacheHitOncePerSession() async throws {
        let resolver = makeResolver()
        let session = UUID()

        try await telemetry.withContext(session: session, trigger: .open) {
            _ = try await resolver.resolveToLocalURL(dotNsName: name)
            _ = try await resolver.resolveToLocalURL(dotNsName: name)
            _ = try await resolver.resolveToLocalURL(dotNsName: name)
        }

        #expect(recorder.events.map { $0.tags["outcome"] } == ["ok", "cache_hit"])
        #expect(recorder.events.last?.extras["dotns.content_hash_ms"] == nil)
        #expect(recorder.events.last?.tags["host.session_id"] == session.uuidString.lowercased())
    }

    @Test func aFailedDownloadIsAnError() async throws {
        let resolver = makeResolver(carFetcher: StubCarFetcher(failures: [URLError(.timedOut)]))

        await #expect(throws: URLError.self) {
            try await telemetry.withContext(session: nil, trigger: .open) {
                try await resolver.stageContent(dotNsName: name)
            }
        }

        #expect(recorder.events.map { $0.tags["outcome"] } == ["error"])
    }

    @Test func aCancelledDownloadReportsNothing() async throws {
        let resolver = makeResolver(carFetcher: StubCarFetcher(failures: [CancellationError()]))

        await #expect(throws: CancellationError.self) {
            try await telemetry.withContext(session: nil, trigger: .open) {
                try await resolver.stageContent(dotNsName: name)
            }
        }

        #expect(recorder.events.isEmpty)
    }

    @Test func anUnreadContentHashIsAnErrorTimedByTheRead() async throws {
        let resolver = makeResolver()

        await #expect(throws: DotNsContractError.self) {
            try await telemetry.withContext(session: nil, trigger: .open) {
                try await resolver.stageContent(dotNsName: "missing.hackm3.dot")
            }
        }

        let event = try #require(recorder.events.first)
        #expect(recorder.events.count == 1)
        #expect(event.tags["outcome"] == "error")
        #expect(event.extras["dotns.content_hash_ms"] == .integer(10))
        #expect(event.extras["duration_ms"] == .integer(10))
    }

    private func makeResolver(carFetcher: StubCarFetcher = StubCarFetcher()) -> DotNsResolver {
        DotNsResolver(
            contractApi: contracts,
            carFetcher: carFetcher,
            contentStorage: InMemoryDotNsContentStorage(),
            contentHashCache: ContentHashCache(),
            telemetry: telemetry
        )
    }
}
