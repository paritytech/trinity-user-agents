import Foundation
import StructuredConcurrencyTestSupport
@testable import polkadot_app

actor MockChainLivenessAnchorProvider: ChainLivenessAnchorProviding {
    private(set) var fetchAnchorCalls: [(target: ChainConnectionTarget, slotCount: Int)] = []
    private var anchorToReturn: ChainLivenessAnchor?
    private var errorToThrow: Error?
    private let gate = TestGate(isClosed: false)

    func setAnchor(_ anchor: ChainLivenessAnchor?) {
        anchorToReturn = anchor
    }

    func setError(_ error: Error?) {
        errorToThrow = error
    }

    /// Parks every subsequent fetch until `release()`, so a test can observe the provider while a
    /// probe is genuinely outstanding. Without this the mock answers immediately and the hold
    /// cannot be exercised at all — a test would only be racing the completion.
    func closeGate() {
        gate.close()
    }

    func openGate() {
        gate.open()
    }

    /// Resumes everything parked so far. Does not change whether new calls park.
    func release() {
        gate.release()
    }

    /// Suspends until at least `count` fetches are parked at the gate.
    func waitForParkedFetches(_ count: Int) async {
        await gate.waitForParked(count)
    }

    func fetchAnchor(
        for target: ChainConnectionTarget,
        slotCount: Int
    ) async throws -> ChainLivenessAnchor {
        fetchAnchorCalls.append((target: target, slotCount: slotCount))

        try await gate.pass()

        if let error = errorToThrow {
            throw error
        }

        guard let anchor = anchorToReturn else {
            throw NSError(domain: "MockChainLivenessAnchorProvider", code: -1)
        }

        return anchor
    }
}
