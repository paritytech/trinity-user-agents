import Foundation
import Products
@testable import polkadot_app

/// Counts latest-build requests and new-build checks, and fails latest-build requests on demand. Every new-build
/// check reports a new build. `started` fires when the runtime starts.
final class MockSPARuntime: SPARuntimeProtocol, @unchecked Sendable {
    private let startURL: URL
    private let latestBuildError: Error?

    let started = TestSignal()

    // Requests arrive from the interactor's task while the test reads the counts.
    private let lock = NSLock()
    private var latestBuildRequests = 0
    private var newBuildRequests = 0

    var latestBuildRequestCount: Int {
        lock.withLock { latestBuildRequests }
    }

    var newBuildRequestCount: Int {
        lock.withLock { newBuildRequests }
    }

    init(startURL: URL, latestBuildError: Error? = nil) {
        self.startURL = startURL
        self.latestBuildError = latestBuildError
    }

    func start(with _: JSEngineProtocol) async throws -> URL {
        started.fire()
        return startURL
    }

    func useLatestBuild() async throws {
        lock.withLock { latestBuildRequests += 1 }

        if let latestBuildError {
            throw latestBuildError
        }
    }

    func hasNewBuild() async throws -> Bool {
        lock.withLock { newBuildRequests += 1 }
        return true
    }

    func dispose() async {}
}
