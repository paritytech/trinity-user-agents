import AsyncExtensions
import Foundation
@testable import Products

/// Serves canned text records and counts reads, so tests can assert on resolution caching.
final class StubDotNsResolver: DotNsResolverProtocol, @unchecked Sendable {
    /// Keyed by `"<name>|<key>"`; an absent entry reads as an empty record.
    var records: [String: String] = [:]
    /// Thrown by every read while set.
    var readError: Error?
    /// Thrown only for these names, so a test can fail one subname and leave its siblings intact.
    var failingNames: Set<String> = []
    /// Content hash returned per name; an absent name reads as `0x00`.
    var contentHashes: [String: String] = [:]

    // Executable subnames are read concurrently, so the tally needs its own guard.
    private let lock = NSLock()
    private var reads: [String] = []

    var readCount: Int {
        lock.withLock { reads.count }
    }

    var readNames: Set<String> {
        lock.withLock { Set(reads) }
    }

    func set(_ value: String, name: String, key: String) {
        records["\(name)|\(key)"] = value
    }

    func getMetadataEntry(dotNsName: String, key: String) async throws -> String? {
        lock.withLock { reads.append(dotNsName) }

        if let readError {
            throw readError
        }

        if failingNames.contains(dotNsName) {
            throw URLError(.timedOut)
        }

        return records["\(dotNsName)|\(key)"]
    }

    func resolveToLocalURL(dotNsName _: String) async throws -> URL {
        URL(fileURLWithPath: "/dev/null")
    }

    func resolveContentHash(dotNsName: String) async throws -> String {
        contentHashes[dotNsName] ?? "0x00"
    }

    func resolveContent(dotNsName: String) async throws -> DotNsResolvedContent {
        try await DotNsResolvedContent(
            contentHash: resolveContentHash(dotNsName: dotNsName),
            localURL: URL(fileURLWithPath: "/dev/null")
        )
    }

    func progressStream(dotNsName _: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        AsyncStream<DotNsLoadProgress> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func clearCache() throws {}
}
