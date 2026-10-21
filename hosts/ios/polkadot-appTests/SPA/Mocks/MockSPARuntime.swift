import Foundation
import Products
@testable import polkadot_app

/// Counts latest-build requests and fails them on demand, so presenter tests can drive Refresh without a runtime.
final class MockSPARuntime: SPARuntimeProtocol, @unchecked Sendable {
    private let startURL: URL
    private let latestBuildError: Error?

    // Requests arrive from the interactor's task while the test reads the count.
    private let lock = NSLock()
    private var latestBuildRequests = 0

    var latestBuildRequestCount: Int {
        lock.withLock { latestBuildRequests }
    }

    init(startURL: URL, latestBuildError: Error? = nil) {
        self.startURL = startURL
        self.latestBuildError = latestBuildError
    }

    func start(with _: JSEngineProtocol) async throws -> URL {
        startURL
    }

    func useLatestBuild() async throws {
        lock.withLock { latestBuildRequests += 1 }

        if let latestBuildError {
            throw latestBuildError
        }
    }

    func dispose() async {}
}
