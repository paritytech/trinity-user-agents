import Foundation
@testable import Products

/// Serves canned text records and counts reads, so tests can assert on resolution caching.
final class StubProductMetadataRepository: ProductMetadataRepositoryProtocol, @unchecked Sendable {
    /// Keyed by `"<name>|<key>"`; an absent entry reads as an empty record.
    var records: [String: String] = [:]
    /// Thrown by every read while set.
    var readError: Error?
    /// Thrown only for these names, so a test can fail one subname and leave its siblings intact.
    var failingNames: Set<String> = []

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

    func fetchMetadata(name: String, key: String) async throws -> String? {
        lock.withLock { reads.append(name) }

        if let readError {
            throw readError
        }

        if failingNames.contains(name) {
            throw URLError(.timedOut)
        }

        return records["\(name)|\(key)"]
    }
}
