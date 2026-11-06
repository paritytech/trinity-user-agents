import Foundation
import IssueMonitoring
import Testing
@testable import Products

struct DotNsResolverTelemetryTests {
    private let name = "app.hackm3.dot"
    private let contracts = StubDotNsContractApi()
    private let recorder = RecordingHostEventRecorder()
    private let sessions = HostSessionRegistry()

    init() {
        contracts.set(contentHash: Data([0x01, 0x70] as [UInt8] + [UInt8](repeating: 0xAB, count: 32)), name: name)
    }

    @Test func aDownloadReportsItsSizeAndContentHashReadUnderTheProductsSession() async throws {
        let session = UUID()
        sessions.open(productID: "hackm3.dot", sessionID: session)

        _ = try await makeResolver().stageContent(dotNsName: name)

        let event = try #require(recorder.events.first)
        #expect(recorder.events.count == 1)
        #expect(event.name == "host.bulletin.fetch")
        #expect(event.tags["outcome"] == "ok")
        #expect(event.tags["dotns.domain"] == nil)
        #expect(event.extras["dotns.domain"] == .string(name))
        #expect(event.extras["bulletin.bytes"] == .integer(Int(StubCarFetcher.downloadedBytes)))
        #expect(event.extras["dotns.content_hash_ms"] == .integer(10))
        #expect(event.extras["duration_ms"] == .integer(10))
        #expect(event.extras["event.monotonic_ms"] == .integer(30))
        #expect(event.extras["host.session_id"] == .string(session.uuidString.lowercased()))
    }

    @Test func contentAlreadyOnDiskIsACacheHit() async throws {
        let resolver = makeResolver()

        _ = try await resolver.stageContent(dotNsName: name)
        _ = try await resolver.stageContent(dotNsName: name)

        #expect(recorder.events.map { $0.tags["outcome"] } == ["ok", "cache_hit"])
        #expect(recorder.events.last?.extras["bulletin.bytes"] == nil)
        #expect(recorder.events.last?.extras["host.session_id"] == nil)
    }

    @Test func aFailedDownloadIsAnError() async throws {
        let resolver = makeResolver(carFetcher: StubCarFetcher(failures: [URLError(.timedOut)]))

        await #expect(throws: URLError.self) {
            try await resolver.stageContent(dotNsName: name)
        }

        #expect(recorder.events.map { $0.tags["outcome"] } == ["error"])
    }

    @Test func anUnreadContentHashReportsNothing() async throws {
        let resolver = makeResolver()

        await #expect(throws: DotNsContractError.self) {
            try await resolver.stageContent(dotNsName: "missing.hackm3.dot")
        }

        #expect(recorder.events.isEmpty)
    }

    private func makeResolver(carFetcher: StubCarFetcher = StubCarFetcher()) -> DotNsResolver {
        DotNsResolver(
            contractApi: contracts,
            carFetcher: carFetcher,
            contentStorage: InMemoryDotNsContentStorage(),
            contentHashCache: ContentHashCache(),
            telemetry: .recording(into: recorder, sessions: sessions)
        )
    }
}
