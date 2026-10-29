import Foundation
import os
@testable import Products

final class StubCarFetcher: CarFetcherProtocol, @unchecked Sendable {
    static let downloadedBytes: Int64 = 1_024
    static let totalBytes: Int64 = 4_096

    private let failures: OSAllocatedUnfairLock<[Error]>
    private let fetches = OSAllocatedUnfairLock(initialState: 0)

    init(failures: [Error] = []) {
        self.failures = OSAllocatedUnfairLock(initialState: failures)
    }

    var fetchCount: Int { fetches.withLock { $0 } }

    func fetchCarToFile(
        contentHash _: Data,
        onProgress: @escaping (_ downloaded: Int64, _ total: Int64?) -> Void
    ) async throws -> URL {
        fetches.withLock { $0 += 1 }

        if let failure = failures.withLock({ $0.isEmpty ? nil : $0.removeFirst() }) {
            throw failure
        }

        onProgress(Self.downloadedBytes, Self.totalBytes)

        return FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    }
}
