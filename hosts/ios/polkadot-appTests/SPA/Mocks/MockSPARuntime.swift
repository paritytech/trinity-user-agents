import Foundation
import Products
@testable import polkadot_app

/// Counts starts, latest-build requests and new-build checks, and fails latest-build requests on demand. A new-build
/// check can be held open by a gate, so a test can act while the check is in flight.
final class MockSPARuntime: SPARuntimeProtocol, @unchecked Sendable {
    private let startURL: URL
    private let latestBuildError: Error?
    private let newBuildResult: Bool
    private let newBuildGate: AsyncStream<Void>?

    // Requests arrive from the interactor's task while the test reads the counts.
    private let lock = NSLock()
    private var starts = 0
    private var latestBuildRequests = 0
    private var newBuildRequests = 0
    private var completedNewBuildChecks = 0

    var startCount: Int {
        lock.withLock { starts }
    }

    var latestBuildRequestCount: Int {
        lock.withLock { latestBuildRequests }
    }

    var newBuildRequestCount: Int {
        lock.withLock { newBuildRequests }
    }

    var completedNewBuildCheckCount: Int {
        lock.withLock { completedNewBuildChecks }
    }

    init(
        startURL: URL,
        latestBuildError: Error? = nil,
        newBuildResult: Bool = false,
        newBuildGate: AsyncStream<Void>? = nil
    ) {
        self.startURL = startURL
        self.latestBuildError = latestBuildError
        self.newBuildResult = newBuildResult
        self.newBuildGate = newBuildGate
    }

    func start(with _: JSEngineProtocol) async throws -> URL {
        lock.withLock { starts += 1 }
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

        if let newBuildGate {
            for await _ in newBuildGate {
                break
            }
        }

        lock.withLock { completedNewBuildChecks += 1 }
        return newBuildResult
    }

    func dispose() async {}
}
